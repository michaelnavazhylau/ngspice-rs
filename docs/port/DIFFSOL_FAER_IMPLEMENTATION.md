# Implemented diffsol/faer integration

**Branch-local M4 update:** [M4_NONLINEAR.md](M4_NONLINEAR.md) adds bounded
nonlinear DC/AC and charge-companion transient. This report remains the historical
linear/BDF delivery; diffsol still rejects nonlinear devices and never consumes
companion stamps. M4 uses faer-backed disposable Newton systems separately.
Local #34/#35 follow-ups add configured/reported DC bias continuation for OP/DC/AC
([DC_CONTINUATION.md](DC_CONTINUATION.md)) and typed R/TEMP/source sweeps
([DC_SWEEPS.md](DC_SWEEPS.md)); explicit continuation controls still reject for
transient, and diffsol remains linear-only.

Implements the bounded rollout in [DIFFSOL_FAER_RECOMMENDATION.md](DIFFSOL_FAER_RECOMMENDATION.md),
without replacing ngspice's trap/Gear semantics or claiming the full M3 milestone.
The companion trap/Gear transient driver is documented separately in
[TRANSIENT.md](TRANSIENT.md).

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
  ||X||inf >= 0.5`, where X contains the computed basis solutions. This
  conservative default may reject otherwise solvable unit-imbalanced systems;
  #46 now provides explicit library scaling wrappers ([EQUILIBRATION.md](EQUILIBRATION.md)).
  #47 retains this policy and documents its aggregate-certification proof caveat
  ([SPARSE_RANK_DIAGNOSTICS.md](SPARSE_RANK_DIAGNOSTICS.md)); formal proof gate #68
  remains unresolved. A passing cutoff is not a newly proved uniqueness theorem.
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

`Circuit::from_netlist` elaborates literal R/C/L/V/I and bounded model-backed R/C/L;
`Registry::with_builtins` now has working factories for those five designators.
Unsupported parameters, expressions, nonlinear factories and elaboration
constructs fail explicitly. Factories preserve the caller's node table on failure.
Top-level resolution and diode input schemas are documented in
[MODEL_SCHEMAS.md](MODEL_SCHEMAS.md). This checkout's bounded passive support
adds recipes, not new solver backends; see [PASSIVE_MODELS.md](PASSIVE_MODELS.md).
All four analysis drivers pass `AnalysisContext` temperatures to immutable
assembly. Repeated runs at different temperatures do not cumulatively adjust
stored values; explicit model TNOM and instance TEMP retain precedence.

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
- `.tran tstep tstop [tstart [tmax]]` with no `backend=`: the SPICE-compatible
  adaptive trapezoidal / Gear-2 **companion driver** (GitHub #26), documented in
  [TRANSIENT.md](TRANSIENT.md): `method=trap` (default) or `gear`, `maxord` 1 or 2,
  local-truncation-error step control, breakpoint landing, C-style output of every
  accepted point.
- `.tran step stop [start [maxstep]] backend=diffsol method=bdf`: separately
  selected adaptive BDF, **not** ngspice trap or fixed Gear-2. `backend=diffsol`
  without `method=bdf`, trap/Gear/maxord/uic/unknown/duplicate options, or an unknown
  backend return an error. Named assignments from tokenized AST cards are
  normalized at the request boundary.

Deck options (#16): `RunConfig::from_netlist(&netlist)?` resolves `.option` cards;
`config.circuit(&netlist)` elaborates at its temperatures, `config.context()` is the
`AnalysisContext`, and `config.request_for(&card)` adds `.options reltol/vntol/abstol`
as `rtol=`/`vntol=`/`abstol=` unless the request states them (request > deck >
defaults). `method`/`maxord`/`chgtol`/`trtol` are forwarded to the companion driver
and make a `backend=diffsol` request fail before simulation. `Circuit::from_netlist`
rejects decks with `.option` cards.

The CLI still exposes inspection/parsing commands, not a new simulation command.
Driver/device coverage text reflects the bounded implementation. APIs above and
this runnable example exercise production simulation:

```sh
cargo run -p spice-analysis --example rc_diffsol --locked
```

## Bounded transient support

The supported structure is the **index-one** linear pencil `E x' + A x = b(t)`
with at least one dynamic degree of freedom (GitHub #28). `E` is split into the
connected components of its petgraph coupling graph; each block is rank-revealed
(1×1 blocks exactly, larger floating/coupled capacitor blocks by dense SVD with
tolerance `64 m ε σ_max`, at most `MAX_MASS_BLOCK` = 512 unknowns). The null
vectors give `N = ker E` and `W = ker Eᵀ`, and the pencil is accepted only when
`Wᵀ A N` passes the numerical sparse rank guard described above. Grounded, floating and coupled
capacitors and index-one RL/RLC/source equations are accepted; higher-index
ideal-source constraints (a source across a capacitor or a floating capacitor),
singular pencils and nonunique nullspaces are rejected. For diagonal `E` this is
exactly the earlier algebraic-block formulation. Integration stays in physical
coordinates, so voltage/current tolerances keep their meaning; diffsol's
zero-diagonal consistent initializer is bypassed (`new_without_initialise`). AC
has no such mass restriction, although it still needs a valid DC point.

The initial state starts from the DC operating point. It is projected onto the
constraints `Wᵀ (b - A x) = 0` for the source's right-hand value at time zero,
moving only along `ker E`, so `E x` (capacitor charges, inductor fluxes) is
preserved. Consistent derivatives use the block pseudo-inverse of `E` plus the
differentiated constraints. Explicit `ic=`/`.ic`/`uic` semantics are not implemented and transient
rejects them. The numeric adapter also rejects inconsistent supplied algebraic
initial conditions instead of silently changing them.

`IndependentSource` exposes validated Constant, right-continuous Step and continuous
Pwl waveforms through the **device API**; numeric PULSE/PWL V/I setters elaborate
to Pwl and `Waveform::PulseDefaults` (#9). C's PULSE defaults (`vsrcload.c`:
TR/TF/PW/PER from `CKTstep`/`CKTfinalTime`, exactly five fields means PW=0) are
resolved when the transient driver calls `LinearSystem::bind_transient_timing`.
Evaluation is `Waveform::value_at(t, Limit::{Left,Right})` and corners are
enumerated lazily by `breakpoints_in(t0, t1)` (never expanded; the BDF driver
consumes at most 100,000 segments). The initial operating point uses the forcing
just before `t=0` (C's MODETRANOP evaluates the waveform, not the DC value), then
projects from the right. Cycles shorter than TR+PW+TF are cut at the period
boundary (a jump). PULSE counts and PWL `r=`/`td=` (#95) stay piecewise linear
between their lazy breakpoints and are supported here; SIN/EXP/SFFM/AM (#94) are
not piecewise linear and are rejected explicitly by this backend (use the
companion driver). Unsupported: `.param` expressions in waveforms. See [FRONTEND_VALUES.md](FRONTEND_VALUES.md). Device-API knot
times must be finite, nonnegative and strictly increasing. DC and AC source excitations remain distinct from the waveform.

For each interval between knots, forcing is preassembled at both endpoints and
is affine; fallible assembly happens outside diffsol callbacks. Custom
`NonLinearOpJacobian`/`LinearOp` operators provide assembled Jacobians and explicit
sparsity; there is no NaN-based discovery or mutable device trial state. The mass
operator implements `out = E*v + beta*out`, including zero-mass algebraic rows.

Integration stops at every source knot. A jump is evaluated from the left for the
old segment, then the state is projected from the right along `ker E` and BDF
history is restarted. Charges and fluxes (`E x`) stay continuous; for a floating
capacitor both plate voltages jump together. Requested endpoint samples use the
right-hand state; interpolation never crosses a source discontinuity. Full initial
derivatives include differentiated algebraic equations (important for RL source
branch currents); diffsol's default zero algebraic derivatives are insufficient
at those restarts with tight branch-current tolerances.

`Device::accept` is called at the accepted initial state, accepted adaptive steps,
and changed algebraic event states, never during Newton trials/rejected steps or
for interpolated plot samples. Acceptance errors propagate. This backend tracks
no companion state: it calls `Circuit::accept_solution`, whose hooks see the
accepted time and `states: None`.

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
The separate trap/Gear companion integrator (`spice_maths::integrator`) provides
order-1/2 coefficients and history operations for the companion driver
([TRANSIENT.md](TRANSIENT.md)); BDF never consumes them.

The #29 numeric-only constrained-RLC prototype is separate from this runtime
adapter: [HIGHER_INDEX_DAE_ADR.md](HIGHER_INDEX_DAE_ADR.md). It does not enable
higher-index integration or change initialization/events. Production waveform,
exact-class mapping, reduced integration/error ownership and corner handling
remain separately gated by #69–#72.

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

The M3 exit-gate decks (GitHub #48, `docs/port/VERIFICATION.md`) additionally run
the floating/coupled-capacitor and PWL/pulse RC/RL/RLC transients on the explicit
`backend=diffsol method=bdf` tokens against the same C goldens: under the
peak-scaled `TRAN_RESTART` bound where C's own backward-Euler restart error exceeds
the pointwise bound, and against closed forms at 7e-7 of device scale in
`crates/spice-analysis/tests/m3_gate.rs`.

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo +1.89.0 test --workspace --locked
cargo +1.89.0 clippy --workspace --all-targets --locked -- -D warnings
NGSPICE_BIN=/path/to/ngspice cargo test -p spice-analysis --test c_linear_reference --locked -- --ignored
NGSPICE_BIN=/path/to/ngspice cargo test -p spice-netlist --test c_reference --locked -- --ignored
```
