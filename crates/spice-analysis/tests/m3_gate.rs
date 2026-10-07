//! M3 exit-gate checks (GitHub #48) through production APIs.
//!
//! The committed conformance decks (`conformance/netlists/`) run through
//! `Parser` -> `RunConfig` -> `companion_transient`/`runner`, exactly like an
//! ordinary `.tran`/`.ac` run, and are judged against
//!
//! * exact closed-form responses (a state-space model advanced with a matrix
//!   exponential over the deck's piecewise-linear drive),
//! * conservation laws at accepted points (KCL, capacitor charge, energy), and
//! * the committed C goldens' own analytic error, which explains why the
//!   physical budgets below are what they are (C's backward-Euler restart step
//!   after each source corner is a first-order error that the C-parity
//!   companion driver reproduces; see `xtask/src/compare.rs`, `TRAN_RESTART`).
//!
//! Budgets are *measured physical error limits*: each constant states the error
//! measured for that deck, the device scale it is relative to, and a safety
//! factor of at most 2. They are not tolerances against C (those live in
//! `cargo xtask golden verify`); no C binary is needed here.
//!
//! The initialized-state decks (`.ic`, `uic`, instance `ic=`; GitHub #27) join
//! the gate in their own section below: closed-form decay from the nonzero
//! state, conserved plate charge on the floating capacitor, and the release of an
//! `.ic` constraint that is applied without `uic`.
//!
//! Still out of scope until the dependencies land: higher-index source
//! constraints (#29), nonlinear charge (M4) and subcircuits (M5).
use std::path::{Path, PathBuf};

use spice_analysis::{
    AnalysisRequest, Plot, RawFile, RunConfig, TransientStats, companion_transient, runner,
};
use spice_core::parse_spice_number;
use spice_netlist::Parser;

fn conformance() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance")
}

/// A fixture deck and its production request.
struct Fixture {
    netlist: spice_netlist::ast::Netlist,
    config: RunConfig,
}

impl Fixture {
    fn load(name: &str) -> Self {
        let netlist = Parser::new()
            .parse_file(conformance().join(format!("netlists/{name}.cir")))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let config = RunConfig::from_netlist(&netlist).unwrap();
        Self { netlist, config }
    }
    fn request(&self) -> AnalysisRequest {
        self.config.request_for(&self.netlist.analyses[0]).unwrap()
    }
    /// An ordinary `.tran` run with optional extra request tokens.
    fn tran(&self, extra: &[&str]) -> spice_core::SpiceResult<(Plot, TransientStats)> {
        let mut request = self.request();
        request
            .arguments
            .extend(extra.iter().map(|s| s.to_string()));
        let mut circuit = self.config.circuit(&self.netlist)?;
        companion_transient(&mut circuit, &request, &self.config.context())
    }
    fn number(&self, index: usize) -> f64 {
        parse_spice_number(self.request().argument(index).unwrap()).unwrap()
    }
    /// Any analysis through the ordinary `runner` (used for `.ac` and for the
    /// explicit diffsol BDF backend selected by extra request tokens).
    fn run(&self, extra: &[&str]) -> spice_core::SpiceResult<Plot> {
        let mut request = self.request();
        request
            .arguments
            .extend(extra.iter().map(|s| s.to_string()));
        let mut circuit = self.config.circuit(&self.netlist)?;
        runner(request.kind)?.run(&mut circuit, &request, &self.config.context())
    }
    fn golden(name: &str) -> Plot {
        let text =
            std::fs::read_to_string(conformance().join(format!("golden/{name}.raw"))).unwrap();
        RawFile::parse(&text).unwrap().plots.remove(0).plot
    }
}

fn column(plot: &Plot, name: &str) -> Vec<f64> {
    plot.column(name)
        .unwrap_or_else(|| panic!("no {name}"))
        .iter()
        .map(|v| v.re)
        .collect()
}

// ---------------------------------------------------------------------------
// Exact piecewise-linear-drive response of a linear state-space model.

/// `x' = A x + B u(t)` with `u` piecewise linear through `knots` (a repeated
/// time is a jump; `u` is held after the last knot and is 0 before the first).
struct Lti {
    a: Vec<Vec<f64>>,
    b: Vec<f64>,
    knots: Vec<(f64, f64)>,
}

fn matmul(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let mut c = vec![vec![0.; b[0].len()]; a.len()];
    for (row, a_row) in c.iter_mut().zip(a) {
        for (a_ik, b_row) in a_row.iter().zip(b) {
            for (c_ij, b_kj) in row.iter_mut().zip(b_row) {
                *c_ij += a_ik * b_kj;
            }
        }
    }
    c
}

/// Matrix exponential by scaling and squaring with a Taylor series
/// (relative accuracy ~1e-15 for the small-norm matrices used here).
fn expm(m: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = m.len();
    let norm = m
        .iter()
        .map(|row| row.iter().map(|v| v.abs()).sum::<f64>())
        .fold(0., f64::max);
    let squarings = (norm / 0.25).log2().ceil().max(0.) as u32;
    let scale = 0.5_f64.powi(squarings as i32);
    let scaled: Vec<Vec<f64>> = m
        .iter()
        .map(|row| row.iter().map(|v| v * scale).collect())
        .collect();
    let mut term: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..n).map(|j| f64::from(i == j)).collect())
        .collect();
    let mut sum = term.clone();
    for k in 1..24 {
        term = matmul(&term, &scaled);
        for row in &mut term {
            for v in row {
                *v /= f64::from(k);
            }
        }
        for i in 0..n {
            for j in 0..n {
                sum[i][j] += term[i][j];
            }
        }
    }
    for _ in 0..squarings {
        sum = matmul(&sum, &sum);
    }
    sum
}

impl Lti {
    /// Input at `t` (right limit at a jump).
    fn input(&self, t: f64) -> f64 {
        let (first, last) = (self.knots[0], self.knots[self.knots.len() - 1]);
        if t < first.0 {
            return 0.;
        }
        if t >= last.0 {
            return last.1;
        }
        let slope = self.slope(t);
        let (t0, u0) = self.knots.iter().rev().find(|k| k.0 <= t).copied().unwrap();
        u0 + slope * (t - t0)
    }

    /// Advances `x` from `t0` to `t1` (`t0 < t1`), splitting at knots.
    fn advance(&self, mut x: Vec<f64>, t0: f64, t1: f64) -> Vec<f64> {
        let n = x.len();
        let mut cuts: Vec<f64> = self
            .knots
            .iter()
            .map(|k| k.0)
            .filter(|&k| k > t0 && k < t1)
            .collect();
        cuts.push(t1);
        let mut start = t0;
        for end in cuts {
            if end <= start {
                continue;
            }
            // Linear input on [start, end]: u(s) = u0 + slope (s - start).
            let mid = 0.5 * (start + end);
            let slope = self.slope(mid);
            let u0 = self.input(mid) - slope * (mid - start);
            let mut m = vec![vec![0.; n + 2]; n + 2];
            for (i, row) in m.iter_mut().enumerate().take(n) {
                row[..n].clone_from_slice(&self.a[i]);
                row[n] = self.b[i] * slope;
                row[n + 1] = self.b[i] * u0;
            }
            m[n][n + 1] = 1.;
            let dt = end - start;
            let scaled: Vec<Vec<f64>> = m
                .iter()
                .map(|r| r.iter().map(|v| v * dt).collect())
                .collect();
            let e = expm(&scaled);
            let mut z = x.clone();
            z.push(0.);
            z.push(1.);
            x = (0..n)
                .map(|i| (0..n + 2).map(|j| e[i][j] * z[j]).sum())
                .collect();
            start = end;
        }
        x
    }

    /// Slope of the input in the knot interval containing `t` (right limit).
    fn slope(&self, t: f64) -> f64 {
        for window in self.knots.windows(2) {
            let ((t0, u0), (t1, u1)) = (window[0], window[1]);
            if t >= t0 && t < t1 {
                return (u1 - u0) / (t1 - t0);
            }
        }
        0.
    }

    /// States at the given increasing times, starting from `x0` at `times[0]`.
    fn states(&self, x0: Vec<f64>, times: &[f64]) -> Vec<Vec<f64>> {
        let mut out = vec![x0.clone()];
        let mut x = x0;
        for pair in times.windows(2) {
            if pair[1] > pair[0] {
                x = self.advance(x, pair[0], pair[1]);
            }
            out.push(x.clone());
        }
        out
    }
}

/// PULSE(v1 v2 td tr tf pw per) knots up to `stop`; `levels = [v1, v2]`,
/// `timing = [td, tr, tf, pw, per]`.
fn pulse_knots(levels: [f64; 2], timing: [f64; 5], stop: f64) -> Vec<(f64, f64)> {
    let ([v1, v2], [td, tr, tf, pw, per]) = (levels, timing);
    let mut knots = vec![(0., v1)];
    let mut start = td;
    while start < stop {
        knots.push((start, v1));
        knots.push((start + tr, v2));
        knots.push((start + tr + pw, v2));
        knots.push((start + tr + pw + tf, v1));
        start += per;
    }
    knots.push((stop + 1., v1));
    knots
}

/// Largest `|signal - exact|` over the accepted points, relative to `scale`.
fn worst(plot: &Plot, name: &str, exact: &[f64]) -> f64 {
    let got = column(plot, name);
    assert_eq!(got.len(), exact.len());
    got.iter()
        .zip(exact)
        .map(|(g, e)| (g - e).abs())
        .fold(0., f64::max)
}

// ---------------------------------------------------------------------------
// Models of the committed decks.

/// Predicts one plot signal from the state and the input.
type Output = Box<dyn Fn(&[f64], f64) -> f64>;

type ModelCase = (&'static str, fn() -> Model, f64);

struct Model {
    lti: Lti,
    /// Signals of the plot predicted by `state -> value` with the input.
    outputs: Vec<(&'static str, Output)>,
    /// Full-scale value of each output (device scale for relative budgets).
    scales: Vec<f64>,
}

fn rl_pulse() -> Model {
    let (r, l) = (100., 10e-3);
    Model {
        lti: Lti {
            a: vec![vec![-r / l]],
            b: vec![1. / l],
            knots: pulse_knots([0., 1.], [20e-6, 1e-6, 1e-6, 200e-6, 500e-6], 600e-6),
        },
        outputs: vec![
            ("i(l1)", Box::new(|x, _| x[0])),
            ("v(out)", Box::new(move |x, u| u - r * x[0])),
            ("i(v1)", Box::new(|x, _| -x[0])),
        ],
        scales: vec![1. / r, 1., 1. / r],
    }
}

fn rc(r: f64, c: f64, knots: Vec<(f64, f64)>) -> Model {
    Model {
        lti: Lti {
            a: vec![vec![-1. / (r * c)]],
            b: vec![1. / (r * c)],
            knots,
        },
        outputs: vec![
            ("v(out)", Box::new(|x, _| x[0])),
            ("i(v1)", Box::new(move |x, u| -(u - x[0]) / r)),
        ],
        scales: vec![1., 1. / r],
    }
}

fn rc_gear() -> Model {
    rc(
        1e3,
        0.1e-6,
        pulse_knots([0., 1.], [100e-6, 10e-6, 10e-6, 300e-6, 800e-6], 1.2e-3),
    )
}

fn rc_pwl() -> Model {
    rc(
        1e3,
        1e-6,
        vec![
            (0., 0.),
            (1e-3, 0.),
            (1.1e-3, 1.),
            (3e-3, 1.),
            (3.1e-3, 0.25),
            (8e-3, 0.25),
        ],
    )
}

fn rlc() -> Model {
    rlc_driven(pulse_knots(
        [0., 1.],
        [50e-6, 20e-6, 20e-6, 400e-6, 1e-3],
        1e-3,
    ))
}

fn rlc_driven(knots: Vec<(f64, f64)>) -> Model {
    let (r, l, c) = (10., 1e-3, 1e-6);
    Model {
        lti: Lti {
            a: vec![vec![-r / l, -1. / l], vec![1. / c, 0.]],
            b: vec![1. / l, 0.],
            knots,
        },
        outputs: vec![
            ("i(l1)", Box::new(|x, _| x[0])),
            ("v(out)", Box::new(|x, _| x[1])),
            ("v(a)", Box::new(move |x, u| u - r * x[0])),
            ("i(v1)", Box::new(|x, _| -x[0])),
        ],
        // Z0 = sqrt(L/C) = 31.6 ohm: peak current is of order 1 V / Z0.
        scales: vec![1. / 31.6, 1., 1., 1. / 31.6],
    }
}

fn ramp_knots() -> Vec<(f64, f64)> {
    vec![(0., 0.), (1e-3, 0.), (1.1e-3, 1.), (8e-3, 1.)]
}

fn floating() -> Model {
    // c1 floats between a and b; the only state is vc = v(a) - v(b).
    let (r1, r2, c) = (1e3, 1e3, 1e-6);
    let tau = (r1 + r2) * c;
    Model {
        lti: Lti {
            a: vec![vec![-1. / tau]],
            b: vec![1. / tau],
            knots: ramp_knots(),
        },
        outputs: vec![
            ("v(b)", Box::new(move |x, u| r2 * (u - x[0]) / (r1 + r2))),
            (
                "v(a)",
                Box::new(move |x, u| r2 * (u - x[0]) / (r1 + r2) + x[0]),
            ),
            ("i(v1)", Box::new(move |x, u| -(u - x[0]) / (r1 + r2))),
        ],
        scales: vec![1., 1., 1. / (r1 + r2)],
    }
}

fn coupled() -> Model {
    // E v' = [(vin - va)/R1 ; -vb/R2], E = [[c1+c12, -c12], [-c12, c2+c12]].
    let (r1, r2, c1, c12, c2) = (1e3, 1e3, 1e-6, 2e-6, 1e-6);
    let det = (c1 + c12) * (c2 + c12) - c12 * c12;
    let inv = [[(c2 + c12) / det, c12 / det], [c12 / det, (c1 + c12) / det]];
    Model {
        lti: Lti {
            a: vec![
                vec![-inv[0][0] / r1, -inv[0][1] / r2],
                vec![-inv[1][0] / r1, -inv[1][1] / r2],
            ],
            b: vec![inv[0][0] / r1, inv[1][0] / r1],
            knots: ramp_knots(),
        },
        outputs: vec![
            ("v(a)", Box::new(|x, _| x[0])),
            ("v(b)", Box::new(|x, _| x[1])),
            ("i(v1)", Box::new(move |x, u| -(u - x[0]) / r1)),
        ],
        scales: vec![1., 1., 1. / r1],
    }
}

/// Exact output at the plot's own time points, per signal, against `plot`.
/// Returns `(name, worst error, error / scale)`.
fn closed_form_errors(plot: &Plot, model: &Model) -> Vec<(&'static str, f64, f64)> {
    closed_form_errors_with(plot, model, 1e-9)
}

/// As [`closed_form_errors`]; `drive_tolerance` bounds how far the plot's
/// `v(in)` may be from the model drive (the BDF backend solves the source
/// constraint only to its Newton tolerance, ~5e-9).
fn closed_form_errors_with(
    plot: &Plot,
    model: &Model,
    drive_tolerance: f64,
) -> Vec<(&'static str, f64, f64)> {
    closed_form_errors_from(plot, model, &vec![0.; model.lti.a.len()], drive_tolerance)
}

/// As [`closed_form_errors_with`] for a run that starts at `t = 0` from the
/// state `x0`. A `uic` plot has no `t = 0` row (its first row is the first
/// accepted step), so the exact state is propagated from `t = 0` and the
/// initial instant is not part of the comparison.
fn closed_form_errors_from(
    plot: &Plot,
    model: &Model,
    x0: &[f64],
    drive_tolerance: f64,
) -> Vec<(&'static str, f64, f64)> {
    let times = column(plot, "time");
    let mut states = if times[0] > 0. {
        let mut from_zero = vec![0.];
        from_zero.extend(&times);
        let mut all = model.lti.states(x0.to_vec(), &from_zero);
        all.remove(0);
        all
    } else {
        model.lti.states(x0.to_vec(), &times)
    };
    states.truncate(times.len());
    let vin = column(plot, "v(in)");
    // The drive itself (accepted left/right limit conventions aside) must be
    // the one this model assumes, or the comparison would be meaningless.
    for (t, v) in times.iter().zip(&vin) {
        assert!(
            (model.lti.input(*t) - v).abs() < drive_tolerance
                || model
                    .lti
                    .knots
                    .windows(2)
                    .any(|w| w[0].0 == w[1].0 && w[0].0 == *t),
            "model drive differs from the deck at t={t:e}: {} vs {v}",
            model.lti.input(*t)
        );
    }
    model
        .outputs
        .iter()
        .zip(&model.scales)
        .map(|((name, f), scale)| {
            let exact: Vec<f64> = states
                .iter()
                .zip(&times)
                .map(|(x, t)| f(x, model.lti.input(*t)))
                .collect();
            let error = worst(plot, name, &exact);
            (*name, error, error / scale)
        })
        .collect()
}

/// One transient fixture: deck, closed-form model, in-run source breakpoints
/// (excluding the stop time) and the measured error budget.
struct Case {
    name: &'static str,
    model: fn() -> Model,
    breakpoints: &'static [f64],
    /// Error of every output relative to its device scale (see [`Model::scales`]).
    /// Measured values (Rust and C agree to 1e-9 of themselves) are in the
    /// comments; each budget is at most twice the measurement.
    budget: f64,
}

const CASES: &[Case] = &[
    // measured 4.8e-5 of the 10 mA / 1 V scale
    Case {
        name: "rl_pulse_tran",
        model: rl_pulse,
        breakpoints: &[20e-6, 21e-6, 221e-6, 222e-6, 520e-6, 521e-6],
        budget: 1e-4,
    },
    // measured 6.8e-5 (Gear-2)
    Case {
        name: "rc_gear_tran",
        model: rc_gear,
        breakpoints: &[100e-6, 110e-6, 410e-6, 420e-6, 900e-6, 910e-6],
        budget: 1.4e-4,
    },
    // measured 7.1e-6
    Case {
        name: "rc_pwl_tran",
        model: rc_pwl,
        breakpoints: &[1e-3, 1.1e-3, 3e-3, 3.1e-3],
        budget: 1.5e-5,
    },
    // measured 1.9e-4 of the 1 V / 31.6 mA scale (trapezoidal)
    Case {
        name: "rlc_series_tran",
        model: rlc,
        breakpoints: &[50e-6, 70e-6, 470e-6, 490e-6],
        budget: 4e-4,
    },
    // measured 7.5e-4 (Gear-2 has a larger error constant than trapezoidal)
    Case {
        name: "rlc_series_gear_tran",
        model: rlc,
        breakpoints: &[50e-6, 70e-6, 470e-6, 490e-6],
        budget: 1.5e-3,
    },
    // measured 1.8e-6
    Case {
        name: "floating_cap_tran",
        model: floating,
        breakpoints: &[1e-3, 1.1e-3],
        budget: 4e-6,
    },
    // measured 3.7e-6
    Case {
        name: "coupled_cap_tran",
        model: coupled,
        breakpoints: &[1e-3, 1.1e-3],
        budget: 8e-6,
    },
];

#[test]
fn companion_and_c_data_match_the_closed_form_within_measured_budgets() {
    for case in CASES {
        let fixture = Fixture::load(case.name);
        let model = (case.model)();
        let (plot, _) = fixture.tran(&[]).unwrap();
        // The committed C data obeys the same physical budget, so the budget is
        // C's own accuracy on this deck, not something fitted to the port.
        for (label, data) in [("Rust", &plot), ("C golden", &Fixture::golden(case.name))] {
            for (signal, error, relative) in closed_form_errors(data, &model) {
                assert!(
                    relative <= case.budget,
                    "{} {label} {signal}: {error:e} is {relative:e} of scale, budget {:e}",
                    case.name,
                    case.budget
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Conservation laws at accepted points.

/// Cumulative trapezoid integral of `y` over `t`.
fn cumulative(t: &[f64], y: &[f64]) -> Vec<f64> {
    let mut out = vec![0.];
    for k in 1..t.len() {
        out.push(out[k - 1] + 0.5 * (y[k] + y[k - 1]) * (t[k] - t[k - 1]));
    }
    out
}

fn peak(values: &[f64]) -> f64 {
    values.iter().fold(0., |m, v| f64::max(m, v.abs()))
}

/// Largest `|supplied - stored - dissipated|` over the run, relative to the
/// largest energy supplied at any instant. `stored` and the series current come
/// from the plot; `R` is the loop resistance.
fn energy_residual(plot: &Plot, series_r: f64, stored: impl Fn(&Plot, usize) -> f64) -> f64 {
    let t = column(plot, "time");
    let vin = column(plot, "v(in)");
    let i: Vec<f64> = column(plot, "i(v1)").iter().map(|v| -v).collect();
    let supplied = cumulative(
        &t,
        &vin.iter().zip(&i).map(|(v, i)| v * i).collect::<Vec<_>>(),
    );
    let dissipated = cumulative(&t, &i.iter().map(|i| series_r * i * i).collect::<Vec<_>>());
    let scale = peak(&supplied);
    (0..t.len())
        .map(|k| (supplied[k] - stored(plot, k) - dissipated[k]).abs())
        .fold(0., f64::max)
        / scale
}

#[test]
fn energy_balance_holds_for_rc_rl_and_rlc_drives() {
    // (fixture, series resistance, stored energy). Budgets are relative to the
    // maximum energy supplied; measured values in the comments.
    type Stored = fn(&Plot, usize) -> f64;
    let cases: [(&str, f64, Stored, f64); 5] = [
        // 1/2 L i^2; measured 2.4e-5.
        (
            "rl_pulse_tran",
            100.,
            |p, k| 0.5 * 10e-3 * column(p, "i(l1)")[k].powi(2),
            5e-5,
        ),
        // 1/2 C v^2; measured 3.9e-5 (Gear-2), 1.3e-5 (PWL)
        (
            "rc_gear_tran",
            1e3,
            |p, k| 0.5 * 0.1e-6 * column(p, "v(out)")[k].powi(2),
            8e-5,
        ),
        (
            "rc_pwl_tran",
            1e3,
            |p, k| 0.5 * 1e-6 * column(p, "v(out)")[k].powi(2),
            2.6e-5,
        ),
        // 1/2 L i^2 + 1/2 C v^2; measured 6.1e-5 (trap), 1.6e-4 (Gear-2)
        (
            "rlc_series_tran",
            10.,
            |p, k| {
                0.5 * 1e-3 * column(p, "i(l1)")[k].powi(2)
                    + 0.5 * 1e-6 * column(p, "v(out)")[k].powi(2)
            },
            1.2e-4,
        ),
        (
            "rlc_series_gear_tran",
            10.,
            |p, k| {
                0.5 * 1e-3 * column(p, "i(l1)")[k].powi(2)
                    + 0.5 * 1e-6 * column(p, "v(out)")[k].powi(2)
            },
            3.2e-4,
        ),
    ];
    for (name, r, stored, budget) in cases {
        let fixture = Fixture::load(name);
        let (plot, _) = fixture.tran(&[]).unwrap();
        let residual = energy_residual(&plot, r, stored);
        let golden = energy_residual(&Fixture::golden(name), r, stored);
        assert!(residual <= budget && golden <= budget, "{name}");
    }
}

#[test]
fn kcl_holds_at_every_accepted_point() {
    // KCL is algebraic in the companion load (capacitor companions carry equal
    // and opposite currents), so it holds to rounding at every accepted point
    // regardless of the integration error.
    for name in ["rlc_series_tran", "rlc_series_gear_tran"] {
        let (plot, _) = Fixture::load(name).tran(&[]).unwrap();
        let (vin, va, il, iv) = (
            column(&plot, "v(in)"),
            column(&plot, "v(a)"),
            column(&plot, "i(l1)"),
            column(&plot, "i(v1)"),
        );
        let scale = peak(&il);
        for k in 0..vin.len() {
            // node a: current through r1 equals the inductor current.
            assert!(
                ((vin[k] - va[k]) / 10. - il[k]).abs() <= 1e-9 * scale,
                "{name} {k}"
            );
            // node in: the source current is the series current (branch sign).
            assert!((iv[k] + il[k]).abs() <= 1e-9 * scale, "{name} {k}");
        }
    }
    // Floating capacitor: the same current flows in r1, through c1 and in r2.
    let (plot, _) = Fixture::load("floating_cap_tran").tran(&[]).unwrap();
    let (vin, va, vb, iv) = (
        column(&plot, "v(in)"),
        column(&plot, "v(a)"),
        column(&plot, "v(b)"),
        column(&plot, "i(v1)"),
    );
    let scale = peak(&iv);
    for k in 0..vin.len() {
        assert!(
            ((vin[k] - va[k]) / 1e3 + iv[k]).abs() <= 1e-9 * scale,
            "node in, {k}"
        );
        assert!((vb[k] / 1e3 + iv[k]).abs() <= 1e-9 * scale, "node b, {k}");
    }
    // Coupled network: the source current is the current in r1.
    let (plot, _) = Fixture::load("coupled_cap_tran").tran(&[]).unwrap();
    let (vin, va, iv) = (
        column(&plot, "v(in)"),
        column(&plot, "v(a)"),
        column(&plot, "i(v1)"),
    );
    for k in 0..vin.len() {
        assert!(
            ((vin[k] - va[k]) / 1e3 + iv[k]).abs() <= 1e-9 * peak(&iv),
            "{k}"
        );
    }
}

/// Largest `|stored(k) - integral|`, relative to `scale`.
fn charge_residual(t: &[f64], stored: &[f64], current_into_storage: &[f64], scale: f64) -> f64 {
    let delivered = cumulative(t, current_into_storage);
    stored
        .iter()
        .zip(&delivered)
        .map(|(q, d)| (q - d).abs())
        .fold(0., f64::max)
        / scale
}

#[test]
fn capacitor_charge_is_conserved_in_floating_and_coupled_networks() {
    for (source, data) in [
        (
            "Rust",
            Fixture::load("floating_cap_tran").tran(&[]).unwrap().0,
        ),
        ("C golden", Fixture::golden("floating_cap_tran")),
    ] {
        // c1's charge equals the charge delivered through r1 (nothing else
        // touches either plate): q = C (va - vb) = integral of -i(v1).
        let t = column(&data, "time");
        let (va, vb, iv) = (
            column(&data, "v(a)"),
            column(&data, "v(b)"),
            column(&data, "i(v1)"),
        );
        let q: Vec<f64> = va.iter().zip(&vb).map(|(a, b)| 1e-6 * (a - b)).collect();
        let delivered: Vec<f64> = iv.iter().map(|i| -i).collect();
        let residual = charge_residual(&t, &q, &delivered, peak(&q));
        assert!(residual <= 1e-3, "{source}: {residual:e}");
    }
    for (source, data) in [
        (
            "Rust",
            Fixture::load("coupled_cap_tran").tran(&[]).unwrap().0,
        ),
        ("C golden", Fixture::golden("coupled_cap_tran")),
    ] {
        // Charge on the capacitor plates tied to each node (c12 couples them):
        // node a holds (c1 + c12) va - c12 vb and receives the current of r1;
        // node b holds -c12 va + (c2 + c12) vb and loses the current of r2.
        let t = column(&data, "time");
        let (va, vb, iv) = (
            column(&data, "v(a)"),
            column(&data, "v(b)"),
            column(&data, "i(v1)"),
        );
        let qa: Vec<f64> = va
            .iter()
            .zip(&vb)
            .map(|(a, b)| 3e-6 * a - 2e-6 * b)
            .collect();
        let qb: Vec<f64> = va
            .iter()
            .zip(&vb)
            .map(|(a, b)| 3e-6 * b - 2e-6 * a)
            .collect();
        let into_a: Vec<f64> = iv.iter().map(|i| -i).collect();
        let out_of_b: Vec<f64> = vb.iter().map(|b| -b / 1e3).collect();
        let residual_a = charge_residual(&t, &qa, &into_a, peak(&qa));
        let residual_b = charge_residual(&t, &qb, &out_of_b, peak(&qa));
        // The charges are not independent of the total either: the net charge on
        // both nodes, c1 va + c2 vb, is what entered through r1 less what left
        // through r2.
        let total: Vec<f64> = va.iter().zip(&vb).map(|(a, b)| 1e-6 * (a + b)).collect();
        let net: Vec<f64> = iv.iter().zip(&vb).map(|(i, b)| -i - b / 1e3).collect();
        let residual_total = charge_residual(&t, &total, &net, peak(&qa));
        eprintln!(
            "coupled {source}: charge residuals {residual_a:e} {residual_b:e} {residual_total:e}"
        );
        // measured 8.3e-8, 7.9e-8 and 1.6e-7 of the peak plate charge.
        for (residual, budget) in [
            (residual_a, 1.7e-7),
            (residual_b, 1.6e-7),
            (residual_total, 3.2e-7),
        ] {
            assert!(residual <= budget, "{source}: {residual:e} > {budget:e}");
        }
    }
    // RLC: the capacitor holds the time integral of the inductor current.
    // Measured 3.0e-6 (trapezoidal: the rule integrates exactly the quantity the
    // trapezoid quadrature measures, apart from the backward-Euler restart
    // steps) and 1.25e-4 (Gear-2, whose charge update is not the trapezoid).
    for (name, budget) in [("rlc_series_tran", 6e-6), ("rlc_series_gear_tran", 2.5e-4)] {
        let data = Fixture::load(name).tran(&[]).unwrap().0;
        let t = column(&data, "time");
        let q: Vec<f64> = column(&data, "v(out)").iter().map(|v| 1e-6 * v).collect();
        let residual = charge_residual(&t, &q, &column(&data, "i(l1)"), peak(&q));
        assert!(residual <= budget, "{name}: {residual:e}");
    }
}

// ---------------------------------------------------------------------------
// Production-API behaviour of the ordinary `.tran` run on the gate decks.

#[test]
fn accepted_points_breakpoints_final_time_and_maxstep_on_every_gate_deck() {
    for case in CASES {
        let fixture = Fixture::load(case.name);
        let (tstep, stop) = (fixture.number(0), fixture.number(1));
        let (plot, stats) = fixture.tran(&[]).unwrap();
        let ts = column(&plot, "time");
        // Only accepted points are output: the DC point plus one per accepted
        // step; nothing is interpolated onto the `.tran` step grid.
        assert_eq!(ts.len(), stats.accepted + 1, "{}", case.name);
        assert_eq!(ts[0], 0., "{}", case.name);
        // The run lands exactly on tstop (bit-for-bit, not within a tolerance).
        assert_eq!(*ts.last().unwrap(), stop, "{}", case.name);
        assert!(ts.windows(2).all(|w| w[1] > w[0]), "{}", case.name);
        // C's default maximum step is min(tstep, tstop / 50).
        let maxstep = tstep.min(stop / 50.);
        assert!(
            stats.max_step <= maxstep * (1. + 1e-9),
            "{} {stats:?}",
            case.name
        );
        assert!(stats.min_step > 0., "{}", case.name);
        // Every source corner inside the run is an accepted point, and the
        // driver counts exactly those (plus the stop time) as breakpoint steps.
        for b in case.breakpoints {
            assert!(
                ts.iter().any(|t| (t - b).abs() <= 1e-15),
                "{}: no accepted point at breakpoint {b:e}",
                case.name
            );
        }
        assert_eq!(
            stats.breakpoints,
            case.breakpoints.len() + 1,
            "{}",
            case.name
        );
    }
}

#[test]
fn an_explicit_maxstep_is_honoured_and_refines_the_answer() {
    let fixture = Fixture::load("rc_pwl_tran");
    let model = rc_pwl();
    let (coarse, coarse_stats) = fixture.tran(&[]).unwrap();
    // `.tran tstep tstop tstart tmax`.
    let (fine, fine_stats) = fixture.tran(&["0", "2.5u"]).unwrap();
    assert!(
        fine_stats.max_step <= 2.5e-6 * (1. + 1e-9),
        "{fine_stats:?}"
    );
    assert!(fine_stats.accepted > 3 * coarse_stats.accepted);
    let error = |p: &Plot| closed_form_errors(p, &model)[0].1;
    assert!(
        error(&fine) < error(&coarse),
        "{} vs {}",
        error(&fine),
        error(&coarse)
    );
}

#[test]
fn tighter_tolerances_reject_steps_and_a_smaller_maxstep_refines_the_answer() {
    for (name, ratio) in [
        ("rlc_series_tran", 3.0..4.6),
        ("rlc_series_gear_tran", 3.0..4.6),
    ] {
        let fixture = Fixture::load(name);
        let model = rlc();
        let v_out = |p: &Plot| closed_form_errors(p, &model)[1].2;
        let (loose, loose_stats) = fixture.tran(&[]).unwrap();
        // Truncation control: the default run is capped by tmax = tstep = 1 us
        // (the controller would take larger steps), so a tighter reltol takes
        // more steps, rejects some, and cannot make the answer worse. Rejected
        // trials never reach the output.
        let (tight, tight_stats) = fixture.tran(&["rtol=1e-6", "trtol=1"]).unwrap();
        assert!(tight_stats.accepted > loose_stats.accepted, "{name}");
        assert!(tight_stats.rejected > loose_stats.rejected, "{name}");
        assert_eq!(tight.point_count(), tight_stats.accepted + 1);
        assert!(v_out(&tight) <= v_out(&loose), "{name}");
        // Step-size refinement (tmax = 1 us is the deck's own cap): halving the
        // maximum step reduces the global error by the order-2 factor, apart from
        // the first-order backward-Euler restart step after each corner.
        let mut errors = vec![v_out(&loose)];
        for tmax in ["0.5u", "0.25u"] {
            let (refined, stats) = fixture.tran(&["0", tmax]).unwrap();
            assert!(stats.max_step <= parse_spice_number(tmax).unwrap() * (1. + 1e-9));
            errors.push(v_out(&refined));
        }
        // Measured ratios 3.98 and 3.99 (trap) and 3.98 and 3.99 (Gear-2).
        assert!(
            ratio.contains(&(errors[0] / errors[1])) && ratio.contains(&(errors[1] / errors[2])),
            "{name}: {errors:?}"
        );
    }
}

#[test]
fn work_and_minimum_step_failures_are_explicit_on_a_gate_deck() {
    let fixture = Fixture::load("rlc_series_tran");
    let error = fixture.tran(&["maxsteps=100"]).unwrap_err().to_string();
    assert!(error.contains("work limit of 100"), "{error}");
    // Rejections count against the same budget as accepted steps.
    let (_, stats) = fixture.tran(&["maxsteps=5000"]).unwrap();
    assert!(stats.accepted + stats.rejected <= 5000);
    // An absurdly strict truncation factor drives the step to delmin: the run
    // fails with the driver's minimum-step error on the RC deck ...
    let error = Fixture::load("rc_pwl_tran")
        .tran(&["trtol=1e-30"])
        .unwrap_err()
        .to_string();
    assert!(error.contains("timestep too small"), "{error}");
    // ... while on the ringing RLC deck (row-equilibrated companion solves stay
    // well conditioned at L/h ~ 1e13) the tiny steps exhaust the work budget
    // first: still an explicit, propagated failure, never a partial plot.
    let error = fixture
        .tran(&["trtol=1e-12", "maxsteps=20000"])
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("timestep too small") || error.contains("work limit of 20000"),
        "{error}"
    );
    // Unsupported selections stay explicit errors rather than being ignored.
    let error = fixture.tran(&["maxord=3"]).unwrap_err().to_string();
    assert!(error.to_lowercase().contains("maxord"), "{error}");
}

#[test]
fn a_fast_edge_is_an_accepted_point_with_its_exact_level() {
    // rc_transient: PULSE(0 5 0 1n 1n 10u 20u); the 1 ns rise ends at 1 ns.
    let fixture = Fixture::load("rc_transient");
    let (plot, stats) = fixture.tran(&[]).unwrap();
    let golden = Fixture::golden("rc_transient");
    for (label, data) in [("Rust", &plot), ("C", &golden)] {
        let ts = column(data, "time");
        let at = ts
            .iter()
            .position(|t| (t - 1e-9).abs() <= 1e-15)
            .unwrap_or_else(|| panic!("{label}: no accepted point at the corner"));
        let vin = column(data, "v(in)");
        // Corner level is exact (right limit = left limit for a ramp end); the
        // point before is mid-ramp and the point after is on the plateau.
        assert!((vin[at] - 5.).abs() <= 1e-9, "{label}: {}", vin[at]);
        assert!(vin[at - 1] < 5. && ts[at - 1] < 1e-9, "{label}");
        assert!((vin[at + 1] - 5.).abs() <= 1e-9, "{label}");
    }
    assert!(stats.breakpoints >= 2);
}

// ---------------------------------------------------------------------------
// The explicit diffsol BDF backend on the same decks (Rust-only syntax).

const BDF: [&str; 2] = ["backend=diffsol", "method=bdf"];

#[test]
fn explicit_bdf_matches_the_closed_form_where_c_trapezoidal_cannot() {
    // (fixture, model, budget relative to scale). BDF (rtol 1e-7) is far more
    // accurate than the C-parity trapezoidal/Gear runs on the same decks: it
    // has no first-order restart step. Measured worst error of any signal:
    // 1.6e-7 (RL), 1.9e-7 (RC PWL), 3.6e-7 (RLC), 1.3e-7 (floating), 1.2e-7
    // (coupled) of the device scale; budgets are at most twice the worst.
    let cases: [ModelCase; 5] = [
        ("rl_pulse_tran", rl_pulse, 7e-7),
        ("rc_pwl_tran", rc_pwl, 7e-7),
        ("rlc_series_tran", rlc, 7e-7),
        // Floating and coupled capacitor networks: index-one DAEs (rank-deficient
        // or nondiagonal mass matrix, GitHub #28).
        ("floating_cap_tran", floating, 7e-7),
        ("coupled_cap_tran", coupled, 7e-7),
    ];
    for (name, model, budget) in cases {
        let fixture = Fixture::load(name);
        let plot = fixture.run(&BDF).unwrap();
        for (signal, _, relative) in closed_form_errors_with(&plot, &model(), 1e-7) {
            assert!(
                relative <= budget,
                "{name} {signal}: {relative:e} > {budget:e}"
            );
        }
    }
}

#[test]
fn explicit_bdf_conserves_charge_and_kcl_in_the_floating_network() {
    let data = Fixture::load("floating_cap_tran").run(&BDF).unwrap();
    let t = column(&data, "time");
    let (va, vb, iv) = (
        column(&data, "v(a)"),
        column(&data, "v(b)"),
        column(&data, "i(v1)"),
    );
    let q: Vec<f64> = va.iter().zip(&vb).map(|(a, b)| 1e-6 * (a - b)).collect();
    let delivered: Vec<f64> = iv.iter().map(|i| -i).collect();
    let residual = charge_residual(&t, &q, &delivered, peak(&q));
    // Measured 2.1e-6 of the peak charge (trapezoid quadrature on the 10 us grid).
    assert!(residual <= 4.2e-6, "{residual:e}");
    // KCL in the series path holds at every requested sample to the Newton
    // tolerance of the BDF solve (measured 1.6e-8 of the peak current).
    let kcl = (0..t.len())
        .map(|k| (vb[k] / 1e3 + iv[k]).abs())
        .fold(0., f64::max)
        / peak(&iv);
    assert!(kcl <= 3.2e-8, "{kcl:e}");
}

// ---------------------------------------------------------------------------
// Complex AC sweep of the series RLC.

/// Series RLC low-pass: `H = 1 / (1 - w^2 L C + j w R C)` for `v(out)`.
fn rlc_transfer(f: f64) -> spice_core::Complex {
    let (r, l, c) = (10., 1e-3, 1e-6);
    let w = 2. * std::f64::consts::PI * f;
    let (re, im) = (1. - w * w * l * c, w * r * c);
    let d = re * re + im * im;
    spice_core::Complex::new(re / d, -im / d)
}

#[test]
fn rlc_ac_sweep_matches_the_closed_form_and_the_c_data() {
    let fixture = Fixture::load("rlc_series_ac");
    let plot = fixture.run(&[]).unwrap();
    let golden = Fixture::golden("rlc_series_ac");
    assert_eq!(plot.point_count(), 100);
    let (r, l, c) = (10., 1e-3, 1e-6);
    for (label, data, budget) in [("Rust", &plot, 1e-9), ("C golden", &golden, 1e-9)] {
        let mut resonance = (0., 0.);
        for k in 0..100 {
            let f = data.value("frequency", k).unwrap();
            assert_eq!(f.im, 0.);
            // `.ac lin 100 100 10k`: f_k = 100 + 100 k.
            assert!(
                (f.re - (100. + 100. * k as f64)).abs() <= 1e-9,
                "{label} {k}"
            );
            let h = rlc_transfer(f.re);
            let w = spice_core::Complex::new(0., 2. * std::f64::consts::PI * f.re);
            let wc = w * spice_core::Complex::real(c);
            let vout = data.value("v(out)", k).unwrap();
            let il = data.value("i(l1)", k).unwrap();
            let iv = data.value("i(v1)", k).unwrap();
            let va = data.value("v(a)", k).unwrap();
            let vin = data.value("v(in)", k).unwrap();
            // The source is 1 V ac with phase 0.
            assert_eq!((vin.re, vin.im), (1., 0.), "{label} {k}");
            for (name, got, want) in [
                ("v(out)", vout, h),
                // i = jwC v(out); the source current has the opposite sign.
                ("i(l1)", il, wc * h),
                ("i(v1)", iv, -(wc * h)),
                // KCL/KVL: v(a) = vin - R i, and v(a) = v(out) + jwL i.
                (
                    "v(a)",
                    va,
                    spice_core::Complex::new(1., 0.) - wc * h * spice_core::Complex::real(r),
                ),
            ] {
                let error = ((got.re - want.re).powi(2) + (got.im - want.im).powi(2)).sqrt();
                let scale = (want.re.powi(2) + want.im.powi(2)).sqrt().max(1e-3 / 31.6);
                assert!(
                    error <= budget * scale.max(1.),
                    "{label} {name} f={}: {got} vs {want}",
                    f.re
                );
            }
            let kvl = va - vout - w * spice_core::Complex::real(l) * il;
            assert!(kvl.re.hypot(kvl.im) <= 1e-9, "{label} KVL {k}");
            let magnitude = vout.re.hypot(vout.im);
            if magnitude > resonance.1 {
                resonance = (f.re, magnitude);
            }
        }
        // The magnitude peaks at f0 sqrt(1 - 2 zeta^2) = 4.91 kHz (f0 = 5.03
        // kHz, zeta = 0.158), i.e. at the 4.9 kHz grid point, with gain
        // 1 / (2 zeta sqrt(1 - zeta^2)) = 3.205.
        assert!((resonance.0 - 4900.).abs() < 1e-6, "{label}");
        assert!(
            (resonance.1 - 3.205).abs() < 0.005,
            "{label}: {resonance:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Initialized-state decks (GitHub #27): `uic` with instance `ic=`, and `.ic`
// without `uic`. Their C goldens were captured with the reference binary like
// every other fixture; none of these decks has a diffsol BDF variant because the
// BDF backend rejects initial conditions (asserted below).

/// A deck's closed-form model, its exact initial state and the measured budget.
struct InitialCase {
    name: &'static str,
    model: fn() -> Model,
    /// State at `t = 0` (the `ic=` values, in the model's state order).
    x0: &'static [f64],
    /// `uic`: no `t = 0` row, the C step breakpoint at the `.tran` step.
    uic: bool,
    /// Largest output error relative to the device scale ([`Model::scales`]).
    budget: f64,
}

fn zero_drive() -> Vec<(f64, f64)> {
    vec![(0., 0.), (1., 0.)]
}

fn rc_discharge() -> Model {
    rc(1e3, 1e-6, zero_drive())
}

fn rlc_decay() -> Model {
    rlc_driven(zero_drive())
}

fn rc_released() -> Model {
    rc(1e3, 1e-6, vec![(0., 1.), (1., 1.)])
}

const INITIAL_CASES: &[InitialCase] = &[
    // v(out) = 2 exp(-t / 1 ms) from c1 ic=2; measured 5.9e-6 of the 1 V / 1 mA scale
    InitialCase {
        name: "rc_ic_uic_tran",
        model: rc_discharge,
        x0: &[2.],
        uic: true,
        budget: 1.2e-5,
    },
    // l1 ic=20m, c1 ic=1 free decay (alpha 5000 /s, zeta 0.158);
    // measured 2.5e-4 of the 1 V / 31.6 mA scale
    InitialCase {
        name: "rlc_ic_uic_tran",
        model: rlc_decay,
        x0: &[20e-3, 1.],
        uic: true,
        budget: 4.9e-4,
    },
    // .ic v(out)=0.25 without uic: released after the initial bias;
    // measured 2.3e-6
    InitialCase {
        name: "rc_ic_node_tran",
        model: rc_released,
        x0: &[0.25],
        uic: false,
        budget: 4.6e-6,
    },
    // c1 ic=2 across a floating capacitor, ramp drive; measured 1.5e-6
    InitialCase {
        name: "floating_cap_ic_tran",
        model: floating,
        x0: &[2.],
        uic: true,
        budget: 3e-6,
    },
];

#[test]
fn initialized_decks_match_the_closed_form_from_their_initial_state() {
    for case in INITIAL_CASES {
        let fixture = Fixture::load(case.name);
        let model = (case.model)();
        let (plot, _) = fixture.tran(&[]).unwrap();
        for (label, data) in [("Rust", &plot), ("C golden", &Fixture::golden(case.name))] {
            for (signal, error, relative) in closed_form_errors_from(data, &model, case.x0, 1e-9) {
                assert!(
                    relative <= case.budget,
                    "{} {label} {signal}: {error:e} is {relative:e} of scale, budget {:e}",
                    case.name,
                    case.budget
                );
            }
        }
    }
}

#[test]
fn uic_decks_have_no_initial_row_and_the_c_step_breakpoint() {
    // C (`dctran.c`): with uic there is no t = 0 row (the first row is the first
    // accepted step), and `CKTsetBreak(CKTstep)` adds a breakpoint at the .tran
    // step. The port reproduces both; ordinary decks still start at t = 0.
    for case in INITIAL_CASES {
        let fixture = Fixture::load(case.name);
        let (tstep, stop) = (fixture.number(0), fixture.number(1));
        let (plot, stats) = fixture.tran(&[]).unwrap();
        let golden = Fixture::golden(case.name);
        for (label, data) in [("Rust", &plot), ("C golden", &golden)] {
            let ts = column(data, "time");
            assert_eq!(*ts.last().unwrap(), stop, "{} {label}", case.name);
            assert!(ts.windows(2).all(|w| w[1] > w[0]), "{} {label}", case.name);
            if case.uic {
                assert!(
                    ts[0] > 0. && ts[0] < tstep / 10.,
                    "{} {label}: {}",
                    case.name,
                    ts[0]
                );
                assert!(
                    ts.iter().any(|t| (t - tstep).abs() <= 1e-15),
                    "{} {label}: no accepted point at the step breakpoint",
                    case.name
                );
            } else {
                assert_eq!(ts[0], 0., "{} {label}", case.name);
            }
        }
        // Rust and C start identically (same first-step rule).
        assert!(
            (column(&plot, "time")[0] - column(&golden, "time")[0]).abs() <= 1e-15,
            "{}",
            case.name
        );
        // The driver counts the step breakpoint (uic only), the stop time and
        // every source corner as breakpoint steps; the ramp drive has two.
        let corners = if case.name == "floating_cap_ic_tran" {
            2
        } else {
            0
        };
        assert_eq!(
            stats.breakpoints,
            1 + usize::from(case.uic) + corners,
            "{}",
            case.name
        );
    }
}

#[test]
fn the_first_uic_row_is_the_declared_initial_state() {
    // The state at the first accepted step (backward Euler from the `ic=`
    // values) differs from the declared state only by the first step's decay.
    let at_first = |name: &str, signal: &str| {
        let plot = Fixture::load(name).tran(&[]).unwrap().0;
        let golden = Fixture::golden(name);
        (column(&plot, signal)[0], column(&golden, signal)[0])
    };
    for (rust, c) in [
        at_first("rc_ic_uic_tran", "v(out)"),
        at_first("rlc_ic_uic_tran", "v(out)"),
    ] {
        assert!((rust - c).abs() <= 1e-9);
    }
    let (v, _) = at_first("rc_ic_uic_tran", "v(out)");
    assert!((v - 2.).abs() <= 2. * 1e-7 / 1e-3 * 1.01, "{v}");
    let (il, _) = at_first("rlc_ic_uic_tran", "i(l1)");
    assert!((il - 20e-3).abs() <= 20e-3 * 1e-3, "{il}");
    let (vc, _) = at_first("rlc_ic_uic_tran", "v(out)");
    assert!((vc - 1.).abs() <= 1e-3, "{vc}");
    // Floating capacitor: the plate voltage v(a) - v(b) starts at ic = 2 V.
    let plot = Fixture::load("floating_cap_ic_tran").tran(&[]).unwrap().0;
    let (va, vb) = (column(&plot, "v(a)")[0], column(&plot, "v(b)")[0]);
    assert!((va - vb - 2.).abs() <= 2e-3, "{}", va - vb);
}

#[test]
fn plate_charge_of_the_floating_capacitor_changes_only_by_the_current_through_r1() {
    // c1 floats between a and b, so the charge on its plates, C (va - vb), starts
    // at C ic = 2 uC and changes only by the charge that flows through r1 (and
    // equally through r2): q(t) = q(t1) + integral of -i(v1) dt. Rust and the C
    // golden obey the same budget; the first row of a uic run is the first
    // accepted step, so the charge is anchored there and, separately, checked
    // against the declared C ic.
    for (label, data) in [
        (
            "Rust",
            Fixture::load("floating_cap_ic_tran").tran(&[]).unwrap().0,
        ),
        ("C golden", Fixture::golden("floating_cap_ic_tran")),
    ] {
        let t = column(&data, "time");
        let (va, vb, iv) = (
            column(&data, "v(a)"),
            column(&data, "v(b)"),
            column(&data, "i(v1)"),
        );
        let q: Vec<f64> = va.iter().zip(&vb).map(|(a, b)| 1e-6 * (a - b)).collect();
        let delivered: Vec<f64> = iv.iter().map(|i| -i).collect();
        let q0 = q[0];
        let anchored: Vec<f64> = q.iter().map(|q| q - q0).collect();
        let residual = charge_residual(&t, &anchored, &delivered, peak(&q));
        let initial = (q[0] - 2e-6).abs() / 2e-6;
        let kcl = (0..t.len())
            .map(|k| (vb[k] / 1e3 + iv[k]).abs())
            .fold(0., f64::max)
            / peak(&iv);
        // Measured 1.39e-6 of the peak plate charge (trapezoid quadrature on the
        // 10 us grid, plus the first step's backward-Euler decay) for both; the
        // first row's charge is 5.0e-5 below C ic (1e-7 s of the 2 ms decay);
        // KCL holds to rounding (1.1e-12 of the peak current at worst).
        assert!(residual <= 2.8e-6, "{label}: {residual:e}");
        assert!(initial <= 1e-4, "{label}: {initial:e}");
        assert!(kcl <= 1e-9, "{label}: {kcl:e}");
    }
}

#[test]
fn an_ic_without_uic_constrains_the_initial_bias_and_is_then_released() {
    let fixture = Fixture::load("rc_ic_node_tran");
    let (plot, _) = fixture.tran(&[]).unwrap();
    let golden = Fixture::golden("rc_ic_node_tran");
    for (label, data) in [("Rust", &plot), ("C golden", &golden)] {
        let (t, vout, iv) = (
            column(data, "time"),
            column(data, "v(out)"),
            column(data, "i(v1)"),
        );
        // The t = 0 row is the constrained bias point: v(out) is the .ic value
        // and the 1 V source supplies (1 - 0.25) / 1 k through r1.
        assert_eq!(t[0], 0., "{label}");
        assert!((vout[0] - 0.25).abs() <= 1e-12, "{label}: {}", vout[0]);
        assert!((iv[0] + 0.75e-3).abs() <= 1e-12, "{label}: {}", iv[0]);
        // Released: v(out) charges toward the source (1 - 0.75 e^(-t/tau), tau =
        // 1 ms), reaching 1 - 0.75 e^-5 at 5 ms instead of staying at 0.25 V.
        let last = *vout.last().unwrap();
        assert!(
            (last - (1. - 0.75 * (-5.0_f64).exp())).abs() <= 1e-5,
            "{label}: {last}"
        );
        assert!(vout.windows(2).all(|w| w[1] >= w[0] - 1e-12), "{label}");
    }
    // Without the .ic the very same deck starts at its DC operating point
    // (v(out) = 1 V, no current) and stays there: the constraint, not the
    // circuit, sets the initial state.
    let text = std::fs::read_to_string(conformance().join("netlists/rc_ic_node_tran.cir"))
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with(".ic"))
        .collect::<Vec<_>>()
        .join("\n");
    let deck = spice_netlist::source::parse_deck_text(Path::new("no_ic.cir"), &text);
    let netlist = Parser::new().parse_deck(&deck).unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let (plain, _) = companion_transient(&mut circuit, &request, &config.context()).unwrap();
    assert!(
        column(&plain, "v(out)")
            .iter()
            .all(|v| (v - 1.).abs() <= 1e-9)
    );
    assert!(column(&plain, "i(v1)").iter().all(|i| i.abs() <= 1e-12));
}

#[test]
fn the_diffsol_bdf_backend_rejects_every_initialized_state_deck_explicitly() {
    for case in INITIAL_CASES {
        let error = Fixture::load(case.name)
            .run(&BDF)
            .expect_err(case.name)
            .to_string();
        let expected = if case.uic { "uic" } else { ".ic" };
        assert!(error.contains(expected), "{}: {error}", case.name);
        assert!(error.contains("companion driver"), "{}: {error}", case.name);
    }
}
