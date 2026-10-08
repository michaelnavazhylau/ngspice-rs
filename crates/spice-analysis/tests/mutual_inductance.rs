//! K mutual inductance through the production analyses (GitHub #80):
//! analytic DC/AC transformer results, an ideal (k = 1) transformer whose
//! turns ratio holds exactly on every transient method and backend, coupled
//! RL decays against the matrix exponential (from `uic` instance `ic=` and
//! from the operating point), the diffsol BDF backend on the coupled mass
//! matrix, and explicit failures.
//!
//! The committed C goldens `transformer_ac`, `transformer_tran`,
//! `transformer_ic_uic_tran` and `transformer_model_uic_tran` are compared by
//! `cargo xtask golden verify`.
use std::path::Path;

use spice_analysis::{AnalysisRequest, Plot, RunConfig, runner};
use spice_core::{Complex, SpiceResult};
use spice_netlist::{Parser, source::parse_deck_text};

/// Runs the deck's only analysis as the CLI does, with `extra` request
/// tokens appended (e.g. the explicit diffsol BDF backend).
fn run_with(body: &str, extra: &[&str]) -> SpiceResult<Plot> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(
        Path::new("k.cir"),
        &format!("mutual\n{body}\n.end\n"),
    ))?;
    let config = RunConfig::from_netlist(&netlist)?;
    let mut request = AnalysisRequest::from(&netlist.analyses[0]);
    request
        .arguments
        .extend(extra.iter().map(|token| (*token).to_owned()));
    let request = config.request(request)?;
    let mut circuit = config.circuit(&netlist)?;
    runner(request.kind)?.run(&mut circuit, &request, &config.context())
}

fn run(body: &str) -> Plot {
    run_with(body, &[]).unwrap_or_else(|error| panic!("{error}\n{body}"))
}

fn column(plot: &Plot, name: &str) -> Vec<f64> {
    plot.column(name)
        .unwrap_or_else(|| panic!("no {name}"))
        .iter()
        .map(|v| v.re)
        .collect()
}

fn complex(plot: &Plot, name: &str, point: usize) -> Complex {
    plot.value(name, point)
        .unwrap_or_else(|| panic!("no {name}"))
}

/// A two-loop transformer: `V` through `r1` into `l1`, `l2` into `rl`.
/// Returns `(I1, I2)` from `[[r1 + jwL1, jwM], [jwM, rl + jwL2]] I = [V, 0]`.
fn transformer(w: f64, r1: f64, rl: f64, l1: f64, l2: f64, m: f64) -> (Complex, Complex) {
    let z11 = Complex::new(r1, w * l1);
    let z12 = Complex::new(0.0, w * m);
    let z22 = Complex::new(rl, w * l2);
    let det = z11 * z22 - z12 * z12;
    let i1 = z22 / det;
    let i2 = -(z12 / det);
    (i1, i2)
}

fn assert_complex(got: Complex, want: Complex, tolerance: f64) {
    let error = (got - want).magnitude();
    assert!(
        error <= tolerance * want.magnitude().max(1e-12),
        "{got} != {want} (error {error:e})"
    );
}

const TRANSFORMER: &str = "vin in 0 dc 1 ac 1\nr1 in p 10\nl1 p 0 1m\nl2 s 0 4m\nrl s 0 100";

#[test]
fn coupled_inductors_are_shorts_at_dc() {
    let p = run(&format!("{TRANSFORMER}\nk1 l1 l2 0.9\n.op"));
    assert_eq!(complex(&p, "v(p)", 0).re, 0.0);
    assert_eq!(complex(&p, "v(s)", 0).re, 0.0);
    assert!((complex(&p, "i(l1)", 0).re - 0.1).abs() < 1e-15);
    assert_eq!(complex(&p, "i(l2)", 0).re, 0.0);
}

#[test]
fn ac_transformers_match_the_two_loop_closed_form() {
    for (cards, k) in [
        ("k1 l1 l2 0.9", 0.9),
        ("k1 l1 l2 k=-0.5", -0.5),
        ("k1 l1 l2 coefficient=1", 1.0),
        // Two K cards on one pair are summed, as C's loads sum them.
        ("k1 l1 l2 0.25\nk2 l2 l1 0.25", 0.5),
    ] {
        let p = run(&format!("{TRANSFORMER}\n{cards}\n.ac dec 5 100 1meg"));
        let m = k * (1e-3_f64 * 4e-3).sqrt();
        for (point, f) in column(&p, "frequency").into_iter().enumerate() {
            let w = 2.0 * std::f64::consts::PI * f;
            let (i1, i2) = transformer(w, 10.0, 100.0, 1e-3, 4e-3, m);
            assert_complex(complex(&p, "i(l1)", point), i1, 1e-12);
            assert_complex(complex(&p, "i(l2)", point), i2, 1e-12);
            assert_complex(
                complex(&p, "v(s)", point),
                Complex::new(-100.0, 0.0) * i2,
                1e-12,
            );
        }
    }
}

#[test]
fn a_multi_inductor_card_equals_one_card_per_pair() {
    let three = "vin in 0 dc 0 ac 1\nr1 in a 1\nla a 0 1m\nlb b 0 4m\nlc c 0 2m\n\
                 rb b 0 10\nrc c 0 5";
    let ac = ".ac dec 5 100 100k";
    let expanded = run(&format!(
        "{three}\nk1 la lb 0.5\nk2 la lc 0.5\nk3 lb lc 0.5\n{ac}"
    ));
    let compact = run(&format!("{three}\nk1 la lb lc 0.5\n{ac}"));
    for name in ["v(b)", "v(c)", "i(la)"] {
        assert_eq!(expanded.column(name), compact.column(name), "{name}");
    }
}

#[test]
fn an_ideal_transformer_keeps_its_turns_ratio_on_every_method_and_backend() {
    // k = 1 (C exempts it from its definiteness warning): v(s) = n v(p) with
    // n = sqrt(L2/L1) = 2 holds algebraically, and in every linear multistep
    // discretization of the coupled fluxes, since flux2 = n flux1 exactly.
    let deck = "vin in 0 pulse(0 1 0 1u 1u 50u 100u)\nrs in p 1\nl1 p 0 1m\nl2 s 0 4m\n\
                k1 l1 l2 1\nrl s 0 100";
    for (options, extra) in [
        ("", &[][..]),
        (".options method=gear", &[][..]),
        ("", &["backend=diffsol", "method=bdf"][..]),
    ] {
        let p = run_with(&format!("{deck}\n{options}\n.tran 1u 200u"), extra)
            .unwrap_or_else(|e| panic!("{options} {extra:?}: {e}"));
        let (vp, vs) = (column(&p, "v(p)"), column(&p, "v(s)"));
        let peak = vp.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        assert!(peak > 0.5, "{options} {extra:?}: peak {peak}");
        let error = vp
            .iter()
            .zip(&vs)
            .map(|(p, s)| (s - 2.0 * p).abs())
            .fold(0.0, f64::max);
        assert!(error < 1e-12, "{options} {extra:?}: ratio error {error:e}");
    }
}

/// `i(t) = exp(-L^-1 R t) i0` for two coupled RL loops, by the eigenvalues
/// of the 2x2 matrix `A = -L^-1 R`.
fn coupled_decay(l1: f64, l2: f64, m: f64, r1: f64, r2: f64, i0: [f64; 2], t: f64) -> [f64; 2] {
    let det = l1 * l2 - m * m;
    // A = -L^-1 R, L^-1 = [[l2, -m], [-m, l1]] / det.
    let a = [
        [-l2 * r1 / det, m * r2 / det],
        [m * r1 / det, -l1 * r2 / det],
    ];
    let trace = a[0][0] + a[1][1];
    let determinant = a[0][0] * a[1][1] - a[0][1] * a[1][0];
    let root = (trace * trace / 4.0 - determinant).sqrt();
    let (s1, s2) = (trace / 2.0 + root, trace / 2.0 - root);
    // exp(At) = (e1 (A - s2 I) - e2 (A - s1 I)) / (s1 - s2).
    let (e1, e2) = ((s1 * t).exp(), (s2 * t).exp());
    let entry = |i: usize, j: usize| {
        let identity = if i == j { 1.0 } else { 0.0 };
        (e1 * (a[i][j] - s2 * identity) - e2 * (a[i][j] - s1 * identity)) / (s1 - s2)
    };
    [
        entry(0, 0) * i0[0] + entry(0, 1) * i0[1],
        entry(1, 0) * i0[0] + entry(1, 1) * i0[1],
    ]
}

fn max_decay_error(p: &Plot, i0: [f64; 2], start: f64) -> f64 {
    let m = 0.7 * (1e-3_f64 * 2e-3).sqrt();
    let (i1, i2) = (column(p, "i(l1)"), column(p, "i(l2)"));
    column(p, "time")
        .into_iter()
        .enumerate()
        .filter(|(_, t)| *t >= start)
        .map(|(n, t)| {
            let want = coupled_decay(1e-3, 2e-3, m, 10.0, 20.0, i0, t - start);
            (i1[n] - want[0]).abs().max((i2[n] - want[1]).abs())
        })
        .fold(0.0, f64::max)
}

#[test]
fn coupled_rl_decay_from_instance_ic_matches_the_matrix_exponential() {
    // uic: the initial fluxes are L1 ic1 + M ic2 and L2 ic2 + M ic1, so the
    // currents start exactly at the ic= values (indload.c MODEUIC).
    let deck = "l1 a 0 1m ic=10m\nr1 a 0 10\nl2 b 0 2m ic=-5m\nr2 b 0 20\nk1 l1 l2 0.7";
    for options in ["", ".options method=gear"] {
        let p = run(&format!("{deck}\n{options}\n.tran 1u 500u uic"));
        let error = max_decay_error(&p, [10e-3, -5e-3], 0.0);
        // Measured 2.7e-7 (trap) and 1.1e-6 A (Gear-2) on the 10 mA scale.
        assert!(error < 2e-6, "{options}: max error {error:e} A");
        println!("coupled RL uic decay {options:?}: max error {error:.3e} A");
    }
    // The diffsol backend keeps rejecting instance ic=/uic explicitly.
    let error = run_with(
        &format!("{deck}\n.tran 1u 500u uic"),
        &["backend=diffsol", "method=bdf"],
    )
    .expect_err("BDF has no uic");
    assert!(error.to_string().contains("uic"), "{error}");
}

#[test]
fn model_backed_coupled_inductors_start_from_their_instance_ic() {
    // The same loops with l1 and l2 model-backed: uic seeds their branch
    // currents from ic= exactly as for literal inductors, and the decay
    // keeps truncation control on the coupled flux.
    let deck = "l1 a 0 lm ic=10m\nr1 a 0 10\nl2 b 0 lm2 ic=-5m\nr2 b 0 20\nk1 l1 l2 0.7\n\
                .model lm l(ind=1m)\n.model lm2 l(ind=2m)";
    let literal = "l1 a 0 1m ic=10m\nr1 a 0 10\nl2 b 0 2m ic=-5m\nr2 b 0 20\nk1 l1 l2 0.7";
    for options in ["", ".options method=gear"] {
        let p = run(&format!("{deck}\n{options}\n.tran 1u 500u uic"));
        let q = run(&format!("{literal}\n{options}\n.tran 1u 500u uic"));
        assert_eq!(column(&p, "time"), column(&q, "time"), "{options}");
        for name in ["i(l1)", "i(l2)"] {
            assert_eq!(column(&p, name), column(&q, name), "{options} {name}");
        }
        let error = max_decay_error(&p, [10e-3, -5e-3], 0.0);
        assert!(error < 2e-6, "{options}: max error {error:e} A");
    }
}

#[test]
fn indverbosity_is_a_documented_no_op() {
    let p = run(&format!(
        "{TRANSFORMER}\nk1 l1 l2 0.5\n.options indverbosity=2\n.ac lin 1 1k 1k"
    ));
    let q = run(&format!("{TRANSFORMER}\nk1 l1 l2 0.5\n.ac lin 1 1k 1k"));
    assert_eq!(complex(&p, "v(s)", 0), complex(&q, "v(s)", 0));
    // It never silences the definiteness rejection.
    let error = run_with(
        &format!("{TRANSFORMER}\nk1 l1 l2 1.5\n.options indverbosity=0\n.op"),
        &[],
    )
    .expect_err("indefinite");
    assert!(
        error.to_string().contains("not positive semidefinite"),
        "{error}"
    );
}

#[test]
fn coupled_rl_decay_from_the_operating_point_matches_the_matrix_exponential() {
    // The source holds 0.1 A in l1 (and none in l2) at the operating point and
    // switches off in 1 ns; afterwards the loops decay with L di/dt = -R i.
    let deck = "v1 in 0 pwl(0 1 1n 0)\nr1 in a 10\nl1 a 0 1m\nl2 b 0 2m\nr2 b 0 20\n\
                k1 l1 l2 0.7";
    for options in ["", ".options method=gear"] {
        let p = run(&format!("{deck}\n{options}\n.tran 1u 500u"));
        assert!((column(&p, "i(l1)")[0] - 0.1).abs() < 1e-15);
        // The flux at the operating point is coupled, so i(l2) starts at 0
        // and the 1 ns switching moves the currents by under 1e-5 A.
        let error = max_decay_error(&p, [0.1, 0.0], 1e-9);
        // Measured 2.1e-6 (trap) and 7.1e-6 A (Gear-2) on the 0.1 A scale.
        assert!(error < 2e-5, "{options}: max error {error:e} A");
        println!("coupled RL decay from the OP {options:?}: max error {error:.3e} A");
    }
}

#[test]
fn the_bdf_backend_integrates_the_coupled_mass_matrix() {
    // The transformer_tran fixture: BDF on the requested grid against a
    // tightly toleranced companion reference (reltol 1e-7, steps <= 10 ns).
    let deck = "vin in 0 pulse(0 1 0 1u 1u 100u 200u)\nrs in p 10\nl1 p 0 1m\nl2 s 0 4m\n\
                k1 l1 l2 0.99\nrl s 0 100";
    let bdf = run_with(
        &format!("{deck}\n.tran 1u 400u"),
        &["backend=diffsol", "method=bdf"],
    )
    .unwrap();
    let reference = run(&format!(
        "{deck}\n.options reltol=1e-7 abstol=1e-15\n.tran 1u 400u 0 10n"
    ));
    let (rt, r1, r2) = (
        column(&reference, "time"),
        column(&reference, "i(l1)"),
        column(&reference, "i(l2)"),
    );
    let at = |values: &[f64], t: f64| {
        let n = rt.partition_point(|x| *x < t).clamp(1, rt.len() - 1);
        let f = (t - rt[n - 1]) / (rt[n] - rt[n - 1]);
        values[n - 1] + f * (values[n] - values[n - 1])
    };
    let (bt, b1, b2) = (
        column(&bdf, "time"),
        column(&bdf, "i(l1)"),
        column(&bdf, "i(l2)"),
    );
    let mut worst: f64 = 0.0;
    for (n, t) in bt.iter().enumerate() {
        worst = worst
            .max((b1[n] - at(&r1, *t)).abs())
            .max((b2[n] - at(&r2, *t)).abs());
    }
    // Measured 1.2e-7 A; the currents peak at tens of mA.
    assert!(worst < 1e-6, "BDF vs reference: {worst:e} A");
    println!("coupled BDF vs tight companion reference: {worst:.3e} A");
}

#[test]
fn invalid_couplings_fail_explicitly_in_every_analysis() {
    for (cards, needle) in [
        ("k1 l1 l2 1.2", "not positive semidefinite"),
        ("k1 l1 l9 0.5", "coupling to non-existent inductor l9"),
        ("k1 l1 rl 0.5", "rl is not an inductor"),
    ] {
        for analysis in [".op", ".ac lin 1 1k 1k", ".tran 1u 10u"] {
            let error = run_with(&format!("{TRANSFORMER}\n{cards}\n{analysis}"), &[])
                .expect_err("invalid coupling");
            assert!(error.to_string().contains(needle), "{analysis}: {error}");
        }
    }
    let error = run_with(&format!("{TRANSFORMER}\nk1 l1 l2\n.op"), &[]).expect_err("no k");
    assert!(
        error.to_string().contains("coupling coefficient"),
        "{error}"
    );
}
