# Opt-in bounded equilibration (#46)

This slice adds maths-library wrappers only. Existing `DenseLu`, `SparseLu`,
`ComplexLu`, storage convenience methods, analysis drivers and deck/CLI defaults
are unchanged. No new dependency, native solver, unsafe code, tolerance change or
KLU algorithm is introduced. The read-only C behavioral reference is
`src/maths/sparse/sputils.c::spScale`: rows scale RHSs, columns scale recovered
solutions. Our max-based factor-selection policy is independent; no C option or
full SPICE parity is claimed.

## API and units

- `EquilibratedDenseLu::new(&Matrix)`
- `EquilibratedSparseLu::new(&SparseMatrix, Option<&SparseSymbolic>)`
- `EquilibratedComplexLu::from_operators(&A, &E, omega)`
- Each owns existing checked LU factors, original assembled coefficients and
  immutable `Equilibration` metadata available through `scaling()`.
- Real `solve(&Vector)` / complex `solve(&[Complex])` accept and return physical
  units. Retain the wrapper for multiple RHSs. Caller mutation cannot stale it.
- `check_residual(rhs, x)` checks a physical-unit candidate against the original
  assembly and returns `BackwardError { row_residuals, row_bounds }`; invalid or
  excessive residuals error instead of returning a passing diagnostic.
- Sparse `symbolic()` retains the existing exact assembled-pattern contract.

For a physical equation `M x = b`, the transformation is

```text
M_s = R M C
b_s = R b
M_s y = b_s
x = C y
```

Positive real factors do not change phases, row/unknown order, current signs or
node/branch namespaces. The module has no device semantics: row units and unknown
units remain the caller's responsibility; matrix row zero is not ground.

## Deterministic bounded policy

1. Validate nonempty square dimensions and finite input coefficients. Fold real
   sparse duplicates and reject duplicate-sum overflow **before** scaling.
2. For complex inputs, separately fold/validate A and E, then assemble
   `A + j omega E` with finite nonnegative omega. `omega = 0` intentionally drops
   dynamic contributions, but still validates E, including duplicate overflow.
3. Choose each row factor as `2^clamp(-floor(log2(row_max)), -512, 512)`, where
   complex maxima use `max(abs(real), abs(imag))`. The exponent is extracted from
   floating-point bits, including subnormal inputs, rather than rounded `log2`.
4. Apply the row factors, then choose column factors by the same rule from the
   row-scaled coefficients. Apply columns once; no iterative convergence policy.
5. All factors are finite, strictly positive, exact powers of two in
   `[2^-512, 2^512]`. Bounds are fixed, public via `MAX_SCALING_EXPONENT`, not a
   configurable policy. Clamp saturation can leave a system unresolved.

All-zero assembled rows or columns fail even for a homogeneous RHS. Each
nonzero real/imaginary component must remain nonzero, normal and finite at every
transform stage: coefficient, frequency product (unless omega is exactly zero),
RHS and recovered solution. Zero/subnormal transformed components are a
conservative destructive-underflow failure; even an intermediate loss that a
later column factor could nominally reverse is rejected. Existing subnormal
inputs may be accepted only if the first transform makes them normal. This
intentionally excludes some otherwise representable tiny physical solutions.

Exact original duplicate cancellations disappear before choosing factors.
Scaling never introduces/removes assembled positions; values cannot silently
underflow to change sparsity. Sparse symbolic reuse therefore checks the exact
original assembled pattern through the existing checked API, regardless of
changes in factor values. Changed/cancelled positions error on reuse. Complex
scaling uses standalone real/imaginary operator snapshots at omega=1 with
`ComplexMatrix`/`ComplexLu`; no alternate LU or rank implementation is present.

## Rank, residuals and limitations

Existing dense pivot and sparse/complex numerical rank checks run on `R M C`,
including their homogeneous-system guards. The tested singular, nonunique and
ideal-source-loop matrices still fail. The inherited sparse/complex policy has
an **aggregate-contraction proof caveat**, documented in
[SPARSE_RANK_DIAGNOSTICS.md](SPARSE_RANK_DIAGNOSTICS.md): per-column residual bounds
and the computed inverse-norm cutoff alone do not formally prove uniqueness.
No singular false acceptance was reproduced in the bounded investigation, but
scaling does not resolve this proof requirement or provide a new certificate.
The independently reviewed certificate proposal is tracked separately in
[#68](https://github.com/michaelnavazhylau/ngspice-rs/issues/68).
Conditioning now refers to transformed coordinates: unscaled unit imbalance
no longer automatically defeats the numerical guard.
**This does not certify forward accuracy for arbitrary
near-dependent matrices.** Dense retains its weaker pivot policy; the benchmark
explicitly demonstrates a near-dependent dense result with 20% forward error
both with and without scaling. Sparse/complex reject that case.

After solving and recovering x, the unchanged normwise criterion is evaluated
against original coefficients and RHS, using physical equation units:

```text
|M x - b|[r] <= 128 * epsilon * n * (sum_c |M[r,c]| * ||x||inf + |b[r]|)
```

Complex magnitudes are used for complex matrices. Arithmetic overflow in the
original residual or scale is an explicit error, even if the scaled solve was
successful. This is the existing row-scaled **normwise**, not componentwise,
backward-error check. In mixed-unit systems it may be permissive for a small
unknown; passing it is not a new absolute physical tolerance or accuracy promise.
Analytic tests additionally check actual original-unit voltage/current values.

Original assembled snapshots use O(nnz) storage (dense worst case O(n²)) in
addition to the existing factors. Each solve allocates transformed vectors and
checks both scaled and original residuals. No refinement, condition estimator,
public scaling-only transform API, complex symbolic reuse or automatic driver
enabling is included. #47 retains production rank policy unchanged; integrated
validation checks these wrappers together with its added guard regressions.

## Validation

At base `803fcb5`, the new `tests/equilibration.rs` contains eight ordinary,
C-independent tests covering mixed row/column voltage/current scales, pivoting,
multiple RHSs, dense/sparse/complex results, complex frequency/phase, immutable
snapshots, exact-pattern reuse, duplicate cancellation before frequency
multiplication, zero rows/columns, source-loop/nonunique/unresolved sparse rank,
empty/dimension/nonfinite/duplicate overflow and coefficient/RHS/solution/residual
range failures. Extremal/subnormal inputs pin finite bounded metadata.

Executed with `CARGO_BUILD_JOBS=2` and private `CARGO_TARGET_DIR=$PWD/target/issue46`:

```sh
cargo test -p spice-maths --locked
cargo test --workspace --locked
cargo +1.89.0 test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo +1.89.0 clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
git diff --check
cargo run --release -p spice-maths --example equilibration_bench --locked
```

Focused maths: **55 passed / 0 failed**. Workspace stable 1.99.0 and MSRV 1.89.0:
**807 passed / 0 failed / 37 ignored** on each. Both Clippy gates and formatting
passed. Logs reside in `/tmp/followup/logs/46/`; live-C/golden verification is left
to the parent integration gate (no fixtures were changed).

## Measured benchmark and acceptance

Reproduce with the release command above. The probe runs n=8 (100 repetitions)
and n=128 (10 repetitions), one untimed warmup per variant, four same-RHS solves
per successful factor. Factor timing includes assembly, metadata and existing
rank diagnostics. Solve timing includes transformations, residuals and vector
allocation. Failed factors/solves and analytic forward failures are counted with
the first reason; zero solve time means not attempted, and `not-measured` accuracy
never means zero error. This is a serial wall-clock microbenchmark on a shared
Apple M5 arm64 host, rustc 1.99.0, not a speedup or statistical performance claim.

Well-scaled real tridiagonal matrices have an analytic infinity-condition bound
of 2.2 from diagonal dominance. Ill-scaled cases apply row ratio `2^40` and column
ratio `2^80` to the same operator; complex cases add a scaled imaginary diagonal.
These isolate unit imbalance, not arbitrary ill-conditioning.

Measured mean microseconds (default → equilibrated), from
`/tmp/followup/logs/46/bench-release-final.log`:

| Case | Backend | Factor µs | Solve µs |
| --- | --- | --- | --- |
| well n=8 | dense | 1.208 → 3.507 | 0.940 → 1.967 |
| well n=8 | sparse | 12.377 → 14.267 | 0.770 → 1.689 |
| well n=8 | complex | 13.162 → 16.504 | 0.796 → 1.435 |
| well n=128 | dense | 152.108 → 186.875 | 39.552 → 44.191 |
| well n=128 | sparse | 582.791 → 536.875 | 4.235 → 8.355 |
| well n=128 | complex | 780.308 → 713.509 | 5.418 → 7.830 |
| ill n=8 | dense | 0.979 reject → 2.547 accept | not attempted → 1.392 |
| ill n=8 | sparse | 9.055 reject → 12.138 accept | not attempted → 1.315 |
| ill n=8 | complex | 13.138 reject → 16.090 accept | not attempted → 1.409 |
| ill n=128 | dense | 93.975 reject → 111.621 accept | not attempted → 26.289 |
| ill n=128 | sparse | 379.263 reject → 362.363 accept | not attempted → 5.318 |
| ill n=128 | complex | 493.192 reject → 482.575 accept | not attempted → 5.474 |

All well-scaled variants pass every factor and solve. Each default ill-scaled
variant rejects every factor (100/100 or 10/10); each equilibrated variant accepts
all and passes 400/400 or 40/40 analytic solves. Worst forward relative errors for
these constructed accepted cases are at most **4.441e-16**. Timing inversions at
n=128 are noise/ordering effects, not evidence that added work speeds up LU.

Failure probes each run ten factor attempts:

- Exact singular: every backend/default rejects 10/10 factors.
- Near-dependent: real dense default/equilibrated factor 10/10, but all 40 solves
  fail the independent analytic forward check with 20% error; sparse/complex
  default/equilibrated reject 10/10 factors. Scaling is not a remedy here.
- Destructive coefficient underflow: all variants reject 10/10 factors; opt-in
  failures identify the transform rather than silently dropping an entry.
- Original residual-scale overflow: dense/sparse both policies factor 10/10 but
  reject 40/40 solves. Complex default rejects 10/10 factors during rank residual
  work; complex opt-in factors 10/10 but rejects 40/40 original-unit solves.

These failure cases remain in the recorded output; no conditioning guard or
physical residual bound was relaxed to improve benchmark acceptance.
