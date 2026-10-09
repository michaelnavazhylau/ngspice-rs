# Sparse rank-diagnostic audit (#47)

## Decision and boundaries

**Retain the production guard unchanged.** This is an audit/test/benchmark
outcome, not an enabled optimization and not a new formal uniqueness certificate.
`SparseLu::new`, `SparseLu::solve`, `ComplexMatrix::factorize` and
`ComplexLu::solve` retain their signatures, owned snapshots, exact assembled-pattern
symbolic reuse (real), finite/overflow checks and original-system backward-error
checks. No production source, dense-factor policy, scaling policy, dependency,
lockfile, C source or golden is changed.

There is no suitable *direct* rank/conditioning certificate exposed by the
selected high-level backend. Complete-basis batching is nevertheless possible
and measurably faster on these samples; it is **not** claimed impossible or
useless. It remains example-only because the audit found a limitation in the
mathematical justification of the existing aggregate guard. Parent explicitly
approved retaining current policy and documenting that limitation rather than
silently changing rejection policy or claiming a theorem. Independent review
must consider that limitation separately from passing regression tests.

## Locked backend evidence

Paths below are relative to the registry source of **faer 0.24.4**, as selected
by `Cargo.lock`, not another installed faer version. faer declares MIT in
`Cargo.toml:30`; no backend algorithm is copied into this port.

| Source location | Actual exposed contract / limitation |
| --- | --- |
| `src/sparse/solvers.rs:29-39,74-91,124-150` | High-level `SymbolicLu` wraps an Arc; `Lu` holds private symbolic/numeric fields. Constructors expose `Result<Lu, LuError>`, not U, numeric pivots, rank or a reciprocal-condition bound. |
| `src/sparse/solvers.rs:331-381` | `SolveCore` accepts a matrix RHS and dispatches to `LuRef`, allocating backend scratch for its column count. This is the public route used by the batching experiment. |
| `src/sparse/linalg/mod.rs:74-82` | `LuError` has `SymbolicSingular { index }` and `Generic(FaerError)`. It is not an unresolved numerical-rank/conditioning certificate. |
| `src/sparse/linalg/lu.rs:1728-1753` | Simplicial pivot selection returns `SymbolicSingular` when no eligible index exists. An eligible *zero-valued* pivot can still reach `recip()`; successful construction is insufficient. |
| `src/sparse/linalg/lu.rs:1355-1436` | Direct `SimplicialLu` exposes `l_factor_unsorted()` and `u_factor_unsorted()`. These are real public low-level factor accessors; it would be inaccurate to say faer exposes no sparse U anywhere. |
| `src/sparse/linalg/lu.rs:76-145` | `SupernodalLu` keeps factor storage private; public methods include dimensions, supernode count and solves, not numeric U/pivot access. |
| `src/sparse/linalg/lu.rs:1816-1902,2096-2186` | Unified `NumericLuRaw` and `NumericLu.raw` are private. `LuRef` exposes row/column permutations and solves, not factors/rank/conditioning. Switching to low-level unified LU alone does not open numeric pivots. |
| `src/sparse/linalg/qr.rs:13,1520-1526,1977-2022`; `src/sparse/solvers.rs:19-27,154-188` | Sparse QR uses no-pivoting QR parameters plus structural fill ordering. Direct simplicial QR exposes R, but the high-level QR wrapper hides factors and does not provide a rank-revealing or condition certificate. A second QR factorization is not an established cheaper replacement. |

A simplicial-only adapter could expose pivots with a different backend selection,
but nonzero pivots certify only exact algebraic invertibility of the computed
factors. Their size/ratio alone does not bound original-matrix conditioning,
factorization error or the norm of the inverse (see the unit-diagonal triangular
regression). It cannot replace the current policy without additional analysis.
Dense SVD/QR as a fallback would change cost/storage and dense-policy boundaries;
none is added. A condition *estimate* or partial/random RHS probe can underestimate
the inverse norm and is not an upper-bound certificate.

Read-only C references: `src/maths/sparse/spfactor.c:403-405` checks a zero pivot;
`sputils.c:1290-1318` describes `spPseudoCondition` as less informative than a
condition number, and `sputils.c:1337-1399` explicitly describes `spCondition` as
an estimate. No SPARSE code was translated, and LGPL KLU was neither copied nor
linked. The existing MIT faer dependency is unchanged; unsafe/native/FFI/new
solver dependencies are absent from the diff.

## What the existing guard does, and the proof caveat

For a square n-by-n matrix A, every basis vector e_j is solved, even if the
user RHS is zero. Let X consist of the computed columns x_j, let
`a_r = sum_c |A_rc|`, `m_j = max_c |X_cj|`, and
`tau = 128 * epsilon * n`. The code checks finite values and each computed
column residual against

```text
|computed(A x_j - e_j)_r| <= tau * (a_r * m_j + delta_rj).
```

It accumulates all inverse absolute row sums and rejects when

```text
tau * ||A||inf * ||X||inf >= 0.5
```

or when norm arithmetic is nonfinite. For complex values the absolute value is
the Euclidean complex magnitude, not a real-only check. The quantity involving
X is a computed inverse-norm proxy until uniqueness is established; calling it
an exact condition number would assume what is being tested.

In exact arithmetic, `||I - A X||inf < 1` would prove A nonsingular: A X has a
Neumann-series inverse and square A consequently has a right inverse. However,
the **present inequalities alone do not establish this contraction**. Even
ignoring residual rounding, summing the per-column bounds gives

```text
sum_j |(A X - I)_rj| <= tau * (a_r * sum_j m_j + 1),
sum_j m_j <= n * ||X||inf.
```

Thus the available bound is `tau * (n * ||A||inf * ||X||inf + 1)`, not
`tau * ||A||inf * ||X||inf`. It may exceed one under the existing cutoff;
roundoff in residual/norm computation also needs an explicit bound. This is a
**missing formal implication**, not a demonstrated production false acceptance.
No rank-deficient false acceptance was observed in this bounded investigation:
source loops, full-pattern dependent matrices and 18 size/row-permutation cycle
Laplacians (real, complex-real and complex-phase versions) all reject. These
finite tests are not a proof for all sparse matrices, nor is the existing module
comment about certification a substitute for one.

### Separately proposed strengthened certificate (not implemented)

Accumulate `sum_j |computed(A x_j - e_j)_r|` in every original-system row while
checking all columns, plus a justified upper bound for residual and absolute-sum
rounding. Require a verified bound on `||I - A X||inf` strictly below one with
an approved safety margin, while retaining the current finite/backward-error and
conditioning rejection checks. This can reuse the existing n solves and
residual traversal with O(n) extra accumulator storage; no dense inverse is
necessary. It adds per-residual magnitude/accumulation work and may reject
systems the present policy accepts. Complex dot-product error, floating-point
underflow, overflow and upward/conservative summation must be covered in the
proof, not ignored. Alternatively using `sum_j m_j` in a conservative bound
could avoid explicit defect storage but is potentially much more rejecting.
Neither route is automatically equivalent to current policy. Implementation
and any enabling/margin choice require explicit approval and mathematical review.

## Deterministic tests

`crates/spice-maths/tests/rank_diagnostics.rs` adds six production-interface
regressions:

- homogeneous ideal-source loops and dependent full-pattern systems;
- near-dependent rejected/accepted cases and genuinely complex phase;
- pivoting, disconnected valid blocks, repeated/zero RHSs and owned snapshots;
- duplicate cancellation, exact-pattern reuse refusal and duplicate overflow;
- ill-conditioned unit-diagonal triangular systems (pivot success is insufficient);
- balanced nullspaces, sizes 3, 8, 17, 65, 129, 257 and three row permutations.

Existing `linear_solvers.rs`, `complex_solver.rs`, storage and DAE tests remain
unchanged and passed. The first new test draft incorrectly expected the
2-by-2 `delta=1e-12` case to reject; the unchanged cutoff permits it
(`tau * condition` approximately 0.227). The rejected-case fixture was corrected
to `1e-13`; no production tolerance was changed. Initial Clippy runs caught two
example/test style issues, both fixed before the final gates.

## Reproducible benchmark

`crates/spice-maths/examples/rank_diagnostics_bench.rs` uses only locked faer and
existing public production APIs. It sets **example-only** sequential faer
parallelism and reports medians of seven repetitions. Every column of the
identity is still tested in increasing order, including final partial batches.
Widths 1, 8, 16 are benchmark variants, **not production settings**.

Each width covers 42 real/complex samples: 32/128/512 node grounded ladders,
row-permuted ladders, sparse 2-D meshes, parallel-source loops, disconnected
near-dependent constraints (accepted/rejected deltas) and ill-scaled diagonal
systems. Actual dimensions are 33..515. Imaginary diagonal nodal terms exercise
complex solves; constraint rows retain their physical zeros. The harness checks
its decision against the production factorization, the expected classification,
and the production homogeneous solve for each sample.

The CSV separates CSC construction, symbolic analysis, numeric factorization,
complete-basis diagnostic and one checked basis RHS. `production_total_us` is the
actual unchanged production assembly + factor + diagnostics, timed separately.
The phase diagnostic is a **mirror kernel**, not instrumented production time:
it omits production Vector conversions/copies and should not be subtracted from
production totals or presented as exact internal profiling. All widths use that
same mirror. Failed diagnostic cases do not perform the timed RHS solve; its
near-zero timer value is not a successful solve measurement. No phase totals
should be inferred by subtracting independently measured medians.

Replay from the assigned checkout, with a private target:

```sh
export CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$PWD/target/issue47"
cargo build -p spice-maths --example rank_diagnostics_bench --release --locked \
  > /tmp/followup/logs/issue47/bench-build-final.log 2>&1
for width in 1 8 16; do
  /usr/bin/time -l target/issue47/release/examples/rank_diagnostics_bench "$width" 7 \
    > /tmp/followup/logs/issue47/bench-final-width-$width.csv \
    2> /tmp/followup/logs/issue47/bench-final-width-$width-memory.log
done
```

Measured on Darwin arm64, rustc 1.99.0, release build, sequential backend.
Representative width-one phase medians, in microseconds:

| Scalar / case / dimension | CSC | Symbolic | Numeric | Diagnostic | Checked RHS | Production total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| real ladder 33 | 0.750 | 2.875 | 2.125 | 20.292 | 0.583 | 28.708 |
| complex ladder 33 | 0.625 | 2.625 | 2.083 | 26.917 | 0.750 | 41.625 |
| real ladder 129 | 2.292 | 9.416 | 4.709 | 284.625 | 2.250 | 310.625 |
| complex ladder 129 | 2.292 | 9.375 | 5.042 | 393.625 | 2.750 | 454.625 |
| real ladder 513 | 8.958 | 37.000 | 16.917 | 4424.125 | 8.791 | 4653.792 |
| complex ladder 513 | 10.125 | 38.166 | 18.417 | 5836.250 | 10.375 | 6503.583 |
| real mesh 513 | 15.042 | 193.333 | 278.625 | 7469.667 | 14.583 | 8129.167 |
| complex mesh 513 | 14.417 | 195.000 | 316.542 | 10117.250 | 18.417 | 11146.458 |

Diagnostic medians for larger cases; full raw CSVs include every size and phase:

| Scalar / case / dimension | width 1 us | width 8 us | width 16 us | Decision at all widths |
| --- | ---: | ---: | ---: | --- |
| real ladder 513 | 4424.125 | 2752.917 | 2755.125 | accept |
| complex ladder 513 | 5836.250 | 3424.250 | 3429.500 | accept |
| real mesh 513 | 7469.667 | 5817.000 | 5831.458 | accept |
| complex mesh 513 | 10117.250 | 8867.083 | 8877.875 | accept |
| real source loop 514 | 5.000 | 12.833 | 25.459 | reject: nonfinite |
| complex source loop 514 | 5918.750 | 3487.667 | 3527.417 | reject: conditioning |
| real near-dependent 515 | 4342.583 | 2723.500 | 2753.750 | reject: conditioning |
| complex near-dependent 515 | 5292.958 | 3320.166 | 3515.500 | reject: conditioning |
| real ill-scaled 513 | 1021.292 | 783.500 | 798.833 | reject: conditioning |
| complex ill-scaled 513 | 2106.500 | 1767.667 | 1855.042 | reject: conditioning |

**Acceptance comparison:** each width yields 24 accepts / 18 rejects, with
zero mismatches against production or expected decisions across all seven
repetitions (294 timed comparisons per width), plus 42 homogeneous comparisons.
Batching improves accepted large ladder diagnostic time about 1.6-1.7x and mesh
about 1.1-1.3x. It also **regresses early real source-loop rejection** about 5x
at width 16: more columns are solved before the first checked column fails.
This is not a demonstrated universally faster strategy.

**Memory:** `/usr/bin/time -l` reports whole-process maximum RSS of 7,258,112 /
7,372,800 / 7,618,560 bytes for widths 1/8/16. These include both scalar types,
all cases, production calls, CSC, backend factors and runtime; they are not
per-factor allocations. The CSV separately reports logical diagnostic peak
payload `n * (width * sizeof(scalar) + sizeof(scalar) + 16)` (width capped at n),
covering batched X, inverse row sums and per-column residual/scale. For n=513,
real payload is 16,416 / 45,144 / 77,976 bytes and complex is 24,624 / 82,080 /
147,744 bytes. Backend scratch, Mat alignment/padding, capacity/allocator overhead
and production wrapper allocations are excluded; exact backend allocated bytes
are not exposed. Complete-basis work remains n RHS columns and O(n*nnz + solve
work); width is bounded and auxiliary storage remains O(n), with a larger constant.

## Validation and unresolved work

Final focused maths: **53 passed / 0 failed** (six new tests). Final workspace
stable and Rust 1.89.0: **805 passed / 0 failed / 37 ignored** each. Workspace
all-target Clippy with `--locked -- -D warnings` passed on both toolchains;
formatting and `git diff --check` passed. Logs are under
`/tmp/followup/logs/issue47/`, with `focused-final.log`,
`workspace-{stable,msrv}-final.log`, `clippy-{stable,msrv}-checked.log`,
`fmt-final.log` and `diff-check.log`. No live C or golden recapture was needed or
performed. This lane does not claim milestone completion or issue-wide formal
certification. Reviewer gate, strengthened-certificate approval/proof and any
production batching decision remain separate follow-ups. The formal certificate
gate is tracked in [#68](https://github.com/michaelnavazhylau/ngspice-rs/issues/68).
Parent integration also corrects existing solver documentation's certification
wording; numeric bodies, checks and thresholds remain unchanged.

## Later change: Newton's balanced fallback (#79)

The guard itself is still unchanged. Behavioural sources (#79) added one
production caller of the opt-in equilibration: when Newton's row-equilibrated
sparse solve fails numerically, `spice-analysis/src/newton.rs` retries it once
with `EquilibratedSparseLu::new_balanced` (Curtis-Reid power-of-two row/column
balancing) and `solve_refined` (at most three refinement rounds, each kept
only if it lowers the componentwise backward error). The rank/conditioning
guard runs on the balanced matrix and every result passes the original-unit
residual check; if both attempts fail, the first error is reported. Systems
the first path accepts are solved exactly as before. The motivating case is a
B source's `~1e32` zero-start slope; see
[BEHAVIOURAL_SOURCES.md](BEHAVIOURAL_SOURCES.md#singular-slopes-at-the-zero-start).
