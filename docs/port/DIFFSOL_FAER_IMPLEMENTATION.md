# Implemented diffsol/faer integration

Implements the bounded rollout in [DIFFSOL_FAER_RECOMMENDATION.md](DIFFSOL_FAER_RECOMMENDATION.md),
without replacing ngspice's trap/Gear semantics or claiming the full M3 milestone.

## Production interfaces

- `spice-maths::SparseMatrix::factorize(&self) -> SparseLu` and
  `Matrix::lu_decompose(&self) -> DenseLu` return **owned snapshots**. Storage
  remains `Clone + PartialEq`, with no backend cache to invalidate. Mutating or
  clearing storage never changes existing factors. Retain factors for repeated RHSs.
- Matrix `solve(&self, rhs)` is a convenience operation which factors the **current**
  assembly; no prior factorization is required. This intentionally replaces the
  old in-place stub contract. Empty systems are errors, not implicit empty solves.
- Sparse triplets are folded before CSC conversion; input and duplicate-sum
  finiteness, dimensions and RHS finiteness are validated. Dense row-major storage
  is converted by indexed construction, never reinterpreted as column-major.
- `SparseLu::new(matrix, Some(previous.symbolic()))` reuses symbolic work only for
  an identical assembled pattern. Cancellation/structural changes return errors.
- Dense LU checks pivots against `epsilon * n * max_abs(A)`. Sparse real and complex
  LU solve every basis RHS for rank diagnostics, including homogeneous systems,
  and reject unresolved conditioning when `128 * epsilon * n * ||A||inf *
  ||inverse(A)||inf >= 0.5`. This conservative, unscaled policy may reject
  otherwise solvable, extremely ill-scaled systems; equilibration is future work.
  Since faer's high-level sparse LU does not expose numeric pivots, the diagnostic
  costs **n extra sparse solves** and O(n) auxiliary storage per factorization.
- Every solve checks finite output and row-scaled normwise backward error:
  `|Ax-b|[r] <= 128 * epsilon * n * (sum_c |A[r,c]| * ||x||inf + |b[r]|)`.
  Residual/norm arithmetic overflow is an explicit numerical error. These are
  diagnostics, not a promise of forward accuracy for arbitrarily conditioned MNA.
- `spice-maths::complex::{ComplexMatrix, ComplexLu}` assembles `A + j omega E`
  and uses faer complex sparse LU with explicit `spice_core::Complex` conversions.
  It never routes AC through a real matrix solve.

## Devices and analyses

`Circuit::from_netlist` elaborates literal scalar R/C/L/V/I AST instances;
`Registry::with_builtins` now has working factories for those five designators.
Unsupported parameters, expressions, model-backed factories and elaboration
constructs fail explicitly. Factories preserve the caller's node table on failure.
The current checkout adds top-level model resolution and bounded diode input
schemas, not new numerical support; see [MODEL_SCHEMAS.md](MODEL_SCHEMAS.md).

`Circuit::rebuild_unknowns` records per-device branch row ranges. Node voltages
come first, then branch currents in device order; ground is omitted. Positive
source/inductor branch current flows from the first terminal to the second.
Plots use `v(node)` and `i(instance)` names. Plot column order follows MNA order,
not C's internal device-vector enumeration; conformance compares by name.

`Device::assemble_linear` and `LinearContext` assemble immutable operators:

```text
E x' + A x = b(t)
inductor row: v+ - v- - L i' = 0
```

Capacitors contribute nodal E, inductors contribute `-L` on their branch mass
entry, resistors contribute conductance to A, and V/I sources contribute branch
constraints and signed forcing. Unsupported devices cannot silently stamp zero.
The real `StampContext` additionally binds branch rows for DC stamping. Dynamic
companion stamps and complex stamps are not conflated with this interface.

Analysis entry point:

```rust
let mut circuit = spice_devices::Circuit::from_netlist(&netlist)?;
let request = spice_analysis::AnalysisRequest::from(&netlist.analyses[0]);
let plot = spice_analysis::runner(request.kind)?.run(
    &mut circuit, &request, &spice_analysis::AnalysisContext::default(),
)?;
```

Implemented analyses:

- `.op`: linear operating point. No nonlinear Newton/gmin/source-stepping policy.
- `.dc source start stop step`: one independent V/I source, either sweep direction,
  with factors reused and no mutation of original source values. Resistor,
  temperature and nested sweeps remain unsupported. At most 100,000 points.
- `.ac lin|dec|oct points start stop`: complex linear RLC equations, requiring a
  valid DC bias point; positive frequencies and at most 100,000 samples.
- `.tran step stop [start [maxstep]] backend=diffsol method=bdf`: separately
  selected adaptive BDF, **not** ngspice trap or fixed Gear-2. Without this explicit
  selection, or with trap/Gear/maxord/uic/unknown/duplicate options, returns an error.
  Named assignments from tokenized AST cards are normalized at the request boundary.

The CLI still exposes inspection/parsing commands, not a new simulation command.
Driver/device coverage text reflects the bounded implementation. APIs above and
this runnable example exercise production simulation:

```sh
cargo run -p spice-analysis --example rc_diffsol --locked
```

## Bounded transient support

The initial supported mass structure is **diagonal E**, with at least one dynamic
row and a nonsingular algebraic A block. This accepts grounded capacitors and
index-one RL/RLC/source equations, including singular mass matrices. Floating or
coupled capacitor operators and higher-index ideal-source constraints are rejected
before diffsol's zero-diagonal initialization partition is used. AC has no such
mass restriction, although it still needs a valid DC point.

The initial state starts from the DC operating point. Algebraic variables are
projected for the source's right-hand value at time zero; differential states are
preserved. Explicit `ic=`/`.ic`/`uic` semantics are not implemented and transient
rejects them. The numeric adapter also rejects inconsistent supplied algebraic
initial conditions instead of silently changing them.

`IndependentSource` exposes validated Constant, right-continuous Step and continuous
Pwl waveforms through the **device API**. Waveform netlist syntax remains a parser
milestone and still errors explicitly. Knot times must be finite, nonnegative and
strictly increasing. DC and AC source excitations remain distinct from the waveform.

For each interval between knots, forcing is preassembled at both endpoints and
is affine; fallible assembly happens outside diffsol callbacks. Custom
`NonLinearOpJacobian`/`LinearOp` operators provide assembled Jacobians and explicit
sparsity; there is no NaN-based discovery or mutable device trial state. The mass
operator implements `out = E*v + beta*out`, including zero-mass algebraic rows.

Integration stops at every source knot. A jump is evaluated from the left for the
old segment, then algebraic states are projected from the right and BDF history is
restarted. Dynamic states stay continuous. Requested endpoint samples use the
right-hand state; interpolation never crosses a source discontinuity. Full initial
derivatives include differentiated algebraic equations (important for RL source
branch currents); diffsol's default zero algebraic derivatives are insufficient
at those restarts with tight branch-current tolerances.

`Device::accept` is called at the accepted initial state, accepted adaptive steps,
and changed algebraic event states, never during Newton trials/rejected steps or
for interpolated plot samples. Acceptance errors propagate.

Requested plot samples and adaptive steps are distinct. The requested maximum
step is enforced by stop times and no-progress/final-time checks. Default options:

| Option | Default |
| --- | --- |
| `rtol` | 1e-7 |
| `vntol` (node-voltage absolute tolerance) | 1e-9 V |
| `abstol` (branch-current absolute tolerance) | 1e-12 A |
| `maxsteps` (whole-run accepted-step budget) | 100,000; allowed range 1–1,000,000 |
| Requested sample limit | 100,001 regular samples plus final endpoint |
| Maximum step if omitted | requested sample step |

Diffsol's own bounded Newton/rejection controls and minimum timestep (1e-13 s)
remain in force; backend failures propagate as `SpiceError::Numerical`. Nonlinear
charge/flux, limiting, DC convergence policies and general DAEs remain deferred.
The existing trap/Gear companion integrator APIs remain explicit stubs.

## Validation and dependencies

Local validation: **245 passed, 5 opt-in external tests ignored, 0 failed** on
both stable (rustc 1.99.0) and Rust 1.89.0. All five external tests were also run
separately against `../ngspice_test/build/src/ngspice` and passed. Clippy with
warnings denied, formatting, the runnable example, and `git diff --check` passed.

The workspace declares Rust **1.89**, tested locally alongside stable, and the
CI test matrix covers both. Locked diffsol 0.17.1/faer 0.24.4 are unchanged;
SuiteSparse/SUNDIALS remain disabled.

Tests cover real/complex production LU, pivoting, repeated RHSs, snapshot mutations,
pattern cancellation, dimension/empty/non-finite/overflow/singular systems;
branch binding and ground elimination; DC blocks/source loops; analytic RC/RL/RLC;
Pwl/Step breakpoints; consistent/inconsistent DAE initialization; accepted-step,
maximum-step and work limits; and AC gain/phase.

Production `.op` solves match committed C RC-divider/RLC goldens at **1e-12 relative
+ 1e-15 absolute** (the absolute scale handles zero currents). Complex AC compares
at **1e-10 relative + 1e-12 absolute**, allowing cancellation roundoff in MNA with
mixed voltage/current scales. Analytic transient tests use **2e-5 V / 2e-6 A**
physical error bounds, comfortably above BDF tolerance and interpolation error.
The live C Pwl RC oracle uses a common requested grid and **2e-5 V** bound, rather
than comparing internal timestep sequences. Existing topology and rawfile/golden
round-trip regressions are preserved.

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo +1.89.0 test --workspace --locked
cargo +1.89.0 clippy --workspace --all-targets --locked -- -D warnings
NGSPICE_BIN=/path/to/ngspice cargo test -p spice-analysis --test c_linear_reference --locked -- --ignored
NGSPICE_BIN=/path/to/ngspice cargo test -p spice-netlist --test c_reference --locked -- --ignored
```
