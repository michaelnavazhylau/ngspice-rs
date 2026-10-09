//! Reproducible acceptance/overhead probe; failures are counted, never filtered.
//! Run in release mode. Timing includes existing rank diagnostics; no claim
//! that row/column normalization repairs arbitrary near dependence is made.
use ngspice_rs::maths::{
    EquilibratedComplexLu, EquilibratedDenseLu, EquilibratedSparseLu, Matrix, SparseMatrix, Vector,
    complex::ComplexMatrix,
};
use ngspice_rs::primitives::{Complex, SpiceResult};
use std::{hint::black_box, time::Instant};

fn measure<T>(
    case: &str,
    backend: &str,
    repeats: usize,
    expected: &[f64],
    build: impl Fn() -> SpiceResult<T>,
    solve: impl Fn(&T) -> SpiceResult<Vec<f64>>,
) {
    // One untimed attempt for every variant, including rejected cases.
    if let Ok(factor) = build() {
        let _ = black_box(solve(&factor));
    }
    let mut factor_time = 0.0;
    let mut solve_time = 0.0;
    let mut factors_ok = 0;
    let mut solves_ok = 0;
    let mut failed = 0;
    let mut worst_error = None::<f64>;
    let mut first_failure = None;
    for _ in 0..repeats {
        let start = Instant::now();
        let factor = black_box(build());
        factor_time += start.elapsed().as_secs_f64();
        match factor {
            Err(e) => {
                failed += 1;
                first_failure.get_or_insert_with(|| e.to_string());
            }
            Ok(factor) => {
                factors_ok += 1;
                for _ in 0..4 {
                    let start = Instant::now();
                    let x = black_box(solve(&factor));
                    solve_time += start.elapsed().as_secs_f64();
                    match x {
                        Err(e) => {
                            failed += 1;
                            first_failure.get_or_insert_with(|| e.to_string());
                        }
                        Ok(x) => {
                            let error = x.iter().zip(expected).fold(0.0_f64, |m, (&x, &e)| {
                                m.max((x - e).abs() / e.abs().max(1e-24))
                            });
                            worst_error = Some(worst_error.unwrap_or(0.0).max(error));
                            if x.len() == expected.len() && error <= 1e-12 {
                                solves_ok += 1;
                            } else {
                                failed += 1;
                                first_failure.get_or_insert_with(|| {
                                    format!("analytic forward check failed: {error:e}")
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    println!(
        "{case},{backend},repeats={repeats},factor_ok={factors_ok},solve_ok={solves_ok},failures={failed},factor_mean_us={:.3},solve_mean_us={:.3},worst_relative_forward_error={},first_failure={}",
        factor_time * 1e6 / repeats as f64,
        if factors_ok == 0 {
            0.0
        } else {
            solve_time * 1e6 / (4 * factors_ok) as f64
        },
        worst_error.map_or_else(|| "not-measured".into(), |v| format!("{v:.3e}")),
        first_failure.as_deref().unwrap_or("none")
    );
}

fn flatten(x: &[Complex]) -> Vec<f64> {
    x.iter().flat_map(|v| [v.re, v.im]).collect()
}

fn run(
    case: &str,
    n: usize,
    ill_scaled: bool,
    failure: Option<&str>,
    repeats: usize,
) -> SpiceResult<()> {
    let mut s = SparseMatrix::new(n, n);
    let mut d = Matrix::zeros(n, n);
    let mut e = SparseMatrix::new(n, n);
    let columns: Vec<_> = (0..n)
        .map(|c| {
            if ill_scaled {
                2.0_f64.powi(if c % 2 == 0 { 40 } else { -40 })
            } else {
                1.0
            }
        })
        .collect();
    for r in 0..n {
        let row = if ill_scaled {
            2.0_f64.powi(if r % 2 == 0 { 20 } else { -20 })
        } else {
            1.0
        };
        for (c, &column) in columns.iter().enumerate() {
            let v = if r == c {
                4.0
            } else if c + 1 == r {
                -1.0
            } else if r + 1 == c {
                0.5
            } else {
                0.0
            };
            s.add(r, c, row * v * column)?;
            d.set(r, c, row * v * column)?;
        }
        e.add(r, r, row * 0.25 * columns[r])?;
    }
    if let Some(kind) = failure {
        s.clear();
        d.clear();
        e.clear();
        let values = match kind {
            "singular" => [1.0, 1.0, 1.0, 1.0],
            "near-dependent" => [1.0, 1.0, 1.0, 1.0 + 1e-15],
            "underflow" => [1e300, 1e-300, 0.0, 1.0],
            _ => [1e308, 0.0, 0.0, 1e308],
        };
        for (i, &v) in values.iter().enumerate() {
            s.add(i / 2, i % 2, v)?;
            d.set(i / 2, i % 2, v)?;
        }
    }
    let expected = Vector::from_slice(&columns.iter().map(|v| 1.0 / v).collect::<Vec<_>>());
    let b = s.mul_vector(&expected)?;
    measure(
        case,
        "dense-default",
        repeats,
        expected.as_slice(),
        || d.lu_decompose(),
        |f| Ok(f.solve(&b)?.as_slice().to_vec()),
    );
    measure(
        case,
        "dense-equilibrated",
        repeats,
        expected.as_slice(),
        || EquilibratedDenseLu::new(&d),
        |f| Ok(f.solve(&b)?.as_slice().to_vec()),
    );
    measure(
        case,
        "sparse-default",
        repeats,
        expected.as_slice(),
        || s.factorize(),
        |f| Ok(f.solve(&b)?.as_slice().to_vec()),
    );
    measure(
        case,
        "sparse-equilibrated",
        repeats,
        expected.as_slice(),
        || EquilibratedSparseLu::new(&s, None),
        |f| Ok(f.solve(&b)?.as_slice().to_vec()),
    );
    let x: Vec<_> = expected
        .as_slice()
        .iter()
        .map(|&v| Complex::new(v, -0.5 * v))
        .collect();
    let mut rhs = vec![Complex::ZERO; n];
    for t in s.triplets() {
        rhs[t.row] = rhs[t.row] + Complex::real(t.value) * x[t.col];
    }
    for t in e.triplets() {
        rhs[t.row] = rhs[t.row] + Complex::imaginary(t.value) * x[t.col];
    }
    let expected = flatten(&x);
    measure(
        case,
        "complex-default",
        repeats,
        &expected,
        || ComplexMatrix::from_operators(&s, &e, 1.0)?.factorize(),
        |f| Ok(flatten(&f.solve(&rhs)?)),
    );
    measure(
        case,
        "complex-equilibrated",
        repeats,
        &expected,
        || EquilibratedComplexLu::from_operators(&s, &e, 1.0),
        |f| Ok(flatten(&f.solve(&rhs)?)),
    );
    Ok(())
}

fn main() -> SpiceResult<()> {
    println!(
        "One untimed warmup per variant. One row/column pass; factor timing includes assembly/scaling/rank diagnostics. Solve timing includes residual checks and allocation; no timing reported for unattempted solves (0). Four RHS solves per accepted factor."
    );
    println!(
        "Well-scaled real tridiagonal condition_inf <= 2.2 by diagonal dominance. Ill-scaled cases apply row ratio 2^40 and column ratio 2^80 to the same operator: unit imbalance, not arbitrary near dependence. No measured condition-number/forward-accuracy guarantee."
    );
    for (n, repeats) in [(8, 100), (128, 10)] {
        run(&format!("well-n{n}"), n, false, None, repeats)?;
        run(&format!("ill-n{n}"), n, true, None, repeats)?;
    }
    for failure in [
        "singular",
        "near-dependent",
        "underflow",
        "residual-overflow",
    ] {
        run(failure, 2, false, Some(failure), 10)?;
    }
    Ok(())
}
