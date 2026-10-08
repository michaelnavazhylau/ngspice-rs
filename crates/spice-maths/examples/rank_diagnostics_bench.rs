//! #47 diagnostic audit harness. Batching is benchmark-only, not a shipped path.
//! Run in release mode; optional arguments: batch width (1..=32), repetitions.
//! Timings separate CSC/symbolic/numeric/diagnostic/checked RHS and production total.
use faer::{
    prelude::*,
    sparse::linalg::solvers::{Lu, SymbolicLu},
    sparse::{SparseColMat, Triplet},
    traits::ComplexField,
};
use spice_core::Complex;
use spice_maths::{SparseMatrix, Vector, complex::ComplexMatrix};
use std::{
    collections::BTreeMap,
    ops::{Add, Mul, Sub},
    time::{Duration, Instant},
};

trait Scalar:
    ComplexField<Real = f64> + Copy + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self>
{
    fn convert(v: Complex) -> Self;
    fn magnitude(self) -> f64;
    fn finite(self) -> bool;
}
impl Scalar for f64 {
    fn convert(v: Complex) -> Self {
        v.re
    }
    fn magnitude(self) -> f64 {
        self.abs()
    }
    fn finite(self) -> bool {
        self.is_finite()
    }
}
impl Scalar for c64 {
    fn convert(v: Complex) -> Self {
        Self::new(v.re, v.im)
    }
    fn magnitude(self) -> f64 {
        self.re.hypot(self.im)
    }
    fn finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }
}

fn operators(nodes: usize, case: &str) -> (SparseMatrix, SparseMatrix) {
    let extras = match case {
        "source_loop" => 2,
        "near_reject" | "near_accept" => 3,
        _ => 1,
    };
    let n = nodes + extras;
    let mut a = SparseMatrix::new(n, n);
    let mut e = SparseMatrix::new(n, n);
    for r in 0..nodes {
        a.add(r, r, 4.).unwrap();
        e.add(r, r, 0.25).unwrap();
        if r + 1 < nodes {
            a.add(r, r + 1, -1.).unwrap();
            a.add(r + 1, r, -1.).unwrap();
        }
    }
    a.add(0, nodes, 1.).unwrap();
    a.add(nodes, 0, 1.).unwrap();
    match case {
        "source_loop" => {
            a.add(0, nodes + 1, 1.).unwrap();
            a.add(nodes + 1, 0, 1.).unwrap();
        }
        "near_reject" | "near_accept" => {
            let delta = if case == "near_reject" { 1e-14 } else { 1e-6 };
            for (r, c, v) in [(0, 0, 1.), (0, 1, 1.), (1, 0, 1.), (1, 1, 1. + delta)] {
                a.add(nodes + 1 + r, nodes + 1 + c, v).unwrap();
            }
        }
        "ill_scaled" => {
            // Valid in exact arithmetic, rejected by the unchanged unscaled guard.
            a.clear();
            e.clear();
            for r in 0..n {
                a.add(r, r, if r + 1 == n { 1e-16 } else { 1. }).unwrap();
            }
        }
        "pivoted" => {
            let original = a;
            a = SparseMatrix::new(n, n);
            for t in original.triplets() {
                a.add((t.row + 1) % n, t.col, t.value).unwrap();
            }
            let original = e;
            e = SparseMatrix::new(n, n);
            for t in original.triplets() {
                e.add((t.row + 1) % n, t.col, t.value).unwrap();
            }
        }
        "mesh" => {
            a.clear();
            let stride = (nodes as f64).sqrt() as usize;
            for r in 0..nodes {
                a.add(r, r, 6.).unwrap();
                for c in [r + 1, r + stride] {
                    if c < nodes && (c != r + 1 || c % stride != 0) {
                        a.add(r, c, -1.).unwrap();
                        a.add(c, r, -1.).unwrap();
                    }
                }
            }
            a.add(0, nodes, 1.).unwrap();
            a.add(nodes, 0, 1.).unwrap();
        }
        _ => {}
    }
    (a, e)
}

fn entries<T: Scalar>(a: &SparseMatrix, e: &SparseMatrix, complex: bool) -> Vec<(usize, usize, T)> {
    let mut values = BTreeMap::new();
    for (operator, imaginary) in [(a, false), (e, true)] {
        if imaginary && !complex {
            continue;
        }
        for t in operator.triplets() {
            let v = if imaginary {
                Complex::imaginary(t.value)
            } else {
                Complex::real(t.value)
            };
            let entry = values.entry((t.row, t.col)).or_insert(Complex::ZERO);
            *entry = *entry + v;
        }
    }
    values
        .into_iter()
        .filter(|(_, v)| *v != Complex::ZERO)
        .map(|((r, c), v)| (r, c, T::convert(v)))
        .collect()
}

// Mirrors the production row-scaled residual, including finite/overflow checks.
fn checked_column<T: Scalar>(
    entries: &[(usize, usize, T)],
    x: &Mat<T>,
    column: usize,
    basis: usize,
) -> Result<(), &'static str> {
    let n = x.nrows();
    let max_x = (0..n)
        .map(|r| x[(r, column)].magnitude())
        .fold(0., f64::max);
    if (0..n).any(|r| !x[(r, column)].finite()) {
        return Err("nonfinite");
    }
    let zero = T::convert(Complex::ZERO);
    let one = T::convert(Complex::real(1.));
    let mut ax = vec![zero; n];
    let mut scale = vec![0.; n];
    scale[basis] = 1.;
    for &(r, c, v) in entries {
        ax[r] += v * x[(c, column)];
        scale[r] += v.magnitude() * max_x;
    }
    for r in 0..n {
        let error = (ax[r] - if r == basis { one } else { zero }).magnitude();
        if !error.is_finite()
            || !scale[r].is_finite()
            || error > 128. * f64::EPSILON * n as f64 * scale[r]
        {
            return Err("residual");
        }
    }
    Ok(())
}

fn diagnostic<T: Scalar>(
    factor: &Lu<usize, T>,
    entries: &[(usize, usize, T)],
    n: usize,
    width: usize,
) -> Result<(), &'static str> {
    let zero = T::convert(Complex::ZERO);
    let one = T::convert(Complex::real(1.));
    let mut inverse_rows = vec![0.; n];
    for start in (0..n).step_by(width) {
        let count = width.min(n - start);
        let mut x = Mat::from_fn(n, count, |r, c| if r == start + c { one } else { zero });
        factor.solve_in_place(x.as_mut());
        for c in 0..count {
            checked_column(entries, &x, c, start + c)?;
            for r in 0..n {
                inverse_rows[r] += x[(r, c)].magnitude();
            }
        }
    }
    let mut rows = vec![0.; n];
    for &(r, _, v) in entries {
        rows[r] += v.magnitude();
    }
    let condition =
        rows.into_iter().fold(0., f64::max) * inverse_rows.into_iter().fold(0., f64::max);
    if !condition.is_finite() || 128. * f64::EPSILON * n as f64 * condition >= 0.5 {
        return Err("conditioning");
    }
    Ok(())
}

fn median(mut times: Vec<Duration>) -> f64 {
    times.sort();
    times[times.len() / 2].as_secs_f64() * 1e6
}

fn run<T: Scalar>(nodes: usize, case: &str, complex: bool, width: usize, reps: usize) {
    let (a, e) = operators(nodes, case);
    let n = a.rows();
    let entries = entries::<T>(&a, &e, complex);
    let mut samples: [Vec<Duration>; 6] = std::array::from_fn(|_| Vec::new());
    let mut decision = None;
    let mut production_decision = None;
    for _ in 0..reps {
        let start = Instant::now();
        let triplets: Vec<_> = entries
            .iter()
            .map(|&(r, c, v)| Triplet::new(r, c, v))
            .collect();
        let csc = SparseColMat::<usize, T>::try_new_from_triplets(n, n, &triplets).unwrap();
        samples[0].push(start.elapsed());
        let start = Instant::now();
        let symbolic = SymbolicLu::try_new(csc.symbolic()).unwrap();
        samples[1].push(start.elapsed());
        let start = Instant::now();
        let factor = Lu::try_new_with_symbolic(symbolic, csc.as_ref());
        samples[2].push(start.elapsed());
        let start = Instant::now();
        let result = match &factor {
            Ok(factor) => diagnostic(factor, &entries, n, width),
            Err(_) => Err("backend"),
        };
        samples[3].push(start.elapsed());
        if let Some(previous) = decision {
            assert_eq!(previous, result, "nondeterministic diagnostic");
        }
        decision = Some(result);
        let start = Instant::now();
        if result.is_ok() {
            let mut x = Mat::from_fn(n, 1, |r, _| {
                T::convert(Complex::real(if r == 0 { 1. } else { 0. }))
            });
            factor.as_ref().unwrap().solve_in_place(x.as_mut());
            checked_column(&entries, &x, 0, 0).unwrap();
            std::hint::black_box(x);
        }
        samples[4].push(start.elapsed());
        let start = Instant::now();
        let accepted = if complex {
            ComplexMatrix::from_operators(&a, &e, 1.)
                .unwrap()
                .factorize()
                .is_ok()
        } else {
            a.factorize().is_ok()
        };
        samples[5].push(start.elapsed());
        assert_eq!(
            accepted,
            result.is_ok(),
            "production/batch decision mismatch for {case} n={n}"
        );
        assert_eq!(
            accepted,
            matches!(case, "ladder" | "pivoted" | "mesh" | "near_accept"),
            "unexpected baseline decision"
        );
        production_decision = Some(accepted);
    }
    // Also exercise a homogeneous RHS through the production API on every sample.
    let zero_accepted = if complex {
        ComplexMatrix::from_operators(&a, &e, 1.)
            .unwrap()
            .factorize()
            .and_then(|f| f.solve(&vec![Complex::ZERO; n]))
            .is_ok()
    } else {
        a.solve(&Vector::zeros(n)).is_ok()
    };
    assert_eq!(zero_accepted, production_decision.unwrap());
    // Logical diagnostic payload, not allocator/backend scratch or total RSS.
    // Batched X + inverse row sums + per-column ax/scale; count never exceeds width.
    let payload_bytes =
        n * (width.min(n) * std::mem::size_of::<T>() + std::mem::size_of::<T>() + 16);
    let times: Vec<_> = samples.into_iter().map(median).collect();
    println!(
        "{},{case},{n},{},{width},{reps},{},{},{},{},{},{},{payload_bytes},{:?}",
        if complex { "complex" } else { "real" },
        entries.len(),
        times[0],
        times[1],
        times[2],
        times[3],
        times[4],
        times[5],
        decision.unwrap()
    );
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        args.len() <= 2,
        "usage: rank_diagnostics_bench [width 1..32] [repetitions 1..100]"
    );
    let width: usize = args
        .first()
        .map_or(1, |s| s.parse().expect("integer width"));
    let reps: usize = args
        .get(1)
        .map_or(5, |s| s.parse().expect("integer repetitions"));
    assert!((1..=32).contains(&width) && (1..=100).contains(&reps));
    // Deterministic backend scheduling for reproducible comparisons, example only.
    faer::set_global_parallelism(faer::Par::Seq);
    println!(
        "scalar,case,n,nnz,width,reps,csc_us,symbolic_us,numeric_us,diagnostic_us,checked_rhs_us,production_total_us,logical_diagnostic_bytes,decision"
    );
    for nodes in [32, 128, 512] {
        for case in [
            "ladder",
            "pivoted",
            "mesh",
            "source_loop",
            "near_reject",
            "near_accept",
            "ill_scaled",
        ] {
            run::<f64>(nodes, case, false, width, reps);
            run::<c64>(nodes, case, true, width, reps);
        }
    }
}
