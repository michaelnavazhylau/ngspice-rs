# diffsol/faer solver integration recommendation

> **Historical design record, not current capability documentation.** The
> baseline below predates the implemented linear engine. Production faer
> real/complex LU, scalar R/C/L/V/I `.op`/`.dc`/`.ac` and restricted explicitly
> selected diffsol BDF now exist. Rust **1.89** is declared and validated; the
> old 1.85 toolchain concern below is resolved. See
> [DIFFSOL_FAER_IMPLEMENTATION.md](DIFFSOL_FAER_IMPLEMENTATION.md) for current
> APIs/limits and the central [TODO.md](../../TODO.md) for remaining work.
>
> Sections below describe findings/recommendations **at the inspection
> baseline**, not outstanding implementation tasks or a fresh test run.

## Scope and inspection baseline

This is an inspection and recommendation, not a solver implementation.

- Dedicated branch: `inspect/diffsol-faer`.
- Dedicated worktree: `../ngspice-rs-diffsol-faer`.
- Base commit: `acfce41d2`, with the original checkout's tracked uncommitted
  changes copied at worktree creation. No changes were made to the original
  checkout. Later changes there are not included in this snapshot.
- `cargo search` reports `diffsol 0.17.1` and `faer 0.24.4` as the latest
  published releases; both are already declared and locked in this workspace.
- `cargo test --workspace --locked`: **221 passed, 4 ignored, 0 failed** on
  `rustc 1.99.0`. Ignored external-oracle tests were not run.
- The two existing tests in `tests/solver_crates.rs` pass:
  faer solves a stamped resistor network, and diffsol integrates exponential
  decay using BDF with `FaerSparseMat<f64>` / `FaerSparseLU<f64>`.
  These exercise dependencies directly, not the production solver interfaces.

## Recommendation

**Use faer for linear algebra first; introduce diffsol as an explicitly
selected, bounded transient backend second.** Do not claim that diffsol's
methods implement ngspice's trapezoidal/Gear semantics.

Keep circuit semantics in `devices` and analysis orchestration in
`analysis`. Keep library-specific numeric adapters in `maths`.
Do not translate the C SPARSE/KLU implementations or rewrite their algorithms
when faer already supplies the required factorization.

### Historical integration points (before implementation)

| Location | Behavior at inspection baseline | Recommended action at that time |
| --- | --- | --- |
| `maths/src/sparse.rs`: `factorize`, `solve` | Both return `NotYetPorted` | Implement real sparse LU with faer CSC storage and owned factors |
| `maths/src/dense.rs`: `lu_decompose`, `solve` | Both return `NotYetPorted` | Implement pivoted dense LU through faer; preserve row-major public storage |
| `maths/src/integrator.rs` | Method/history types; coefficient, integration and prediction stubs | Retain the companion-model contract; add a separate diffsol time-integration adapter |
| `devices/src/rlc.rs`, `registry.rs` | R/C/L stamps and all registered factories are stubs | Implement R/V/I and branch-row binding before claiming end-to-end DC; then C/L assembly |
| `devices/src/traits.rs`: `StampContext`, `Device` | Real matrix/RHS stamping and an accepted-point hook | Extend for dynamic equations and complex assembly without conflating trial evaluation with acceptance |
| `devices/src/circuit.rs`: `rebuild_unknowns` | Reserves branch rows but discards returned starts | Record per-device branch-row ranges so V sources and inductors can stamp their equations |
| `analysis/src/analysis.rs` | `.op`, `.dc`, `.ac`, `.tran` are all stubs | Implement drivers incrementally; preserve explicit errors for unsupported cases |

At that baseline there was no completed Rust Newton loop or transient driver
to replace. Today the bounded BDF driver exists, but a SPICE nonlinear DC Newton
loop and trap/Gear companions remain pending. The C analysis/device/integration
routines remain behavioral references.

## Phase 1: production faer linear solves

1. Preserve `SparseMatrix`'s triplet stamping, duplicate addition and public
   matrix/vector operations. Convert assembled entries with
   `SparseColMat::<usize, f64>::try_new_from_triplets` and factor with `sp_lu`.
2. Prefer an owned factorization type separate from matrix storage. This
   avoids adding backend cache semantics to the existing `Clone + PartialEq`
   matrix and avoids stale factors after stamping. If retaining the existing
   `factorize(&mut self)` / `solve(&self)` contract, invalidate cached numeric
   factors on **every** mutation (`add`, `clear`, duplicate folding, etc.).
   Dense storage additionally exposes `data_mut`, `set`, `add_to` and `fill`.
   Define solve-before-factorization behavior explicitly.
3. For repeated Newton steps or sweeps, separate symbolic from numeric work:
   `faer::sparse::linalg::solvers::SymbolicLu::try_new` followed by
   `Lu::try_new_with_symbolic`. Reuse symbolic factors only for an identical
   CSC pattern. Current duplicate folding removes exact-zero entries, so a
   value cancellation can change that pattern. A later fixed-pattern assembler
   should retain structural slots independent of instantaneous values.
4. Validate square dimensions, RHS length, finite assembled coefficients
   (including overflow from duplicate summation), and finite RHS values
   before calling backend operations that can assert dimensions. Define an
   explicit policy for empty systems.
5. Map construction/factorization errors to `SpiceError::Numerical`, preserving
   their context. Do not assume successful LU implies numerical nonsingularity:
   faer's sparse `LuError` includes structural singularity but is not a complete
   numeric rank/conditioning check. Reject non-finite solutions and check a
   scaled backward residual against the original assembled matrix. Establish
   a pivot/rank diagnostic for degenerate systems, including homogeneous RHS;
   residual checks alone do not prove uniqueness.
6. Convert dense row-major matrices deliberately (e.g. indexed construction)
   before `partial_piv_lu`; do not reinterpret their slice as column-major.
   Add dense singular-pivot diagnostics as well.
7. Implement linear `.op` only after R/V/I factories, branch allocation and
   stamping work. Build plots from the solved MNA unknowns, omitting ground
   from the system and reporting branch currents with documented orientation.

Use general LU, not Cholesky: voltage-source/inductor branch constraints make
MNA matrices indefinite, and future controlled/nonlinear stamps can be
nonsymmetric. Graph connectedness is not a nonsingularity test. Keep the
copied petgraph APIs: row 0 is an ordinary MNA unknown, not ground; circuit
incidence connectivity does not necessarily describe DC conduction.

## Phase 2: a bounded diffsol transient backend

Start with linear RLC circuits and independent sources whose assembled
capacitive/inductive operator is state-independent:

```text
E * dx/dt + A * x = b(t)
f(t, x) = b(t) - A * x
J_f * v = -A * v
```

Use `OdeBuilder::<FaerSparseMat<f64>>::new()`, `rhs_implicit`, `mass`, `init`,
`rtol`, component-wise `atol`, and `bdf::<FaerSparseLU<f64>>()`. The mass
closure must implement **`out = E * v + beta * out`**, including algebraic
rows, rather than ignoring `beta`. Keep the signs of inductor equations
consistent with branch-current orientation.

Do not feed timestep-dependent companion stamps into this formulation and
also let diffsol integrate them: that would discretize the dynamics twice.
Introduce a device equation-assembly interface for static residual/Jacobian,
dynamic operator, sources and known breakpoints. Prefer immutable/preassembled
operators for the initial linear implementation; do not hide mutable
`Device::stamp` calls behind closures without defining trial-state ownership
and error propagation. Builder callbacks return values through output buffers,
not `SpiceResult`, so fallible assembly must happen outside callbacks or use
an explicitly designed adapter error channel.

### Required guardrails

- **Method compatibility:** diffsol BDF has maximum order 5 in
  `ode_solver/bdf_state.rs`; the public port type advertises Gear orders 1–6.
  Its adaptive BDF is not automatically ngspice fixed Gear-2. TR-BDF2 is a
  composite method, not the default trapezoidal rule. Expose backend/method
  selection distinctly and reject unsupported combinations rather than
  silently remapping `.option method=trap|gear` or `maxord`. Preserving the
  existing M3 roadmap requires separate trapezoidal/Gear-2 companion-model
  work or an explicit roadmap change.
- **DAE initialization:** singular mass matrices are supported, but automatic
  consistent initialization partitions variables by zero mass diagonal
  (`ode_solver/state.rs`). Validate this against actual MNA structures;
  floating capacitor subnetworks and higher-index ideal-source constraints
  are not covered by the exponential-decay smoke test. Start from an operating
  point, define `.ic`/`uic` behavior, and diagnose unsupported structures.
- **Nonlinear charge:** the mass callback takes `(v, p, t, beta, out)`, not the
  current solution. General `d q(x)/dt + f(x,t) = 0` device charge cannot be
  represented by simply evaluating a state-dependent capacitance there.
  Defer such devices until a charge-state reformulation or a compatible
  residual-based approach is demonstrated, with its required Jacobians.
- **Sparsity:** default closure-based discovery uses NaN probes and can fail
  on state-dependent branches. For production devices, provide explicit
  structural sparsity and assembled Jacobians through custom operators
  (`NonLinearOpJacobian` / `LinearOp`) rather than assuming discovery works.
- **Acceptance and discontinuities:** use `set_stop_time` at source breakpoints;
  inspect step stop reasons, update/restart integration history as required,
  and call `Device::accept` only for accepted points. Never interpolate across
  discontinuities or commit state during rejected trials.
- **Output and limits:** distinguish requested plot samples from adaptive
  internal steps. Bound work/progress and final-time handling, honor requested
  maximum step constraints, and propagate integration failures. A voltage
  tolerance and a branch-current tolerance require different component-wise
  absolute tolerances; do not reuse one absolute value for every unknown.
- **DC nonlinear convergence:** keep future device limiting, source/gmin
  stepping and operating-point convergence in the analysis layer. diffsol's
  internal transient Newton method does not supply these SPICE policies.

## Phase 3: AC and broader validation

AC needs complex `A + j*omega*E` assembly and faer complex sparse LU, not diffsol
integration. The inspection-baseline stamping interface was real-only; the recommendation
was to add a complex assembly representation and explicit scalar conversions at the backend
boundary. Do not force an AC implementation through `SparseMatrix<f64>`.

## Historical toolchain issue (resolved by the integration)

At inspection the workspace declared Rust **1.85**, but `cargo metadata --locked`
reported:

| Resolved dependency | Declared minimum Rust |
| --- | --- |
| `nalgebra 0.35.0` | 1.89.0 |
| `safe_arch 1.2.0` | 1.89 |
| `wide 1.7.1` | 1.89 |

`cargo tree -i nalgebra` confirms the diffsol-la dependency path. Selecting only
`diffsol`'s `faer` feature does not remove it: diffsol 0.17.1 explicitly enables
both faer and nalgebra on its la/nl dependencies. Recommend raising the declared
workspace MSRV to at least 1.89 and testing it in CI, subject to the project's
support policy. If 1.85 is mandatory, investigate an upstream feature fix or
an older compatible stack; do not assume disabling top-level defaults fixes it.
The original inspection did not install or test Rust 1.85/1.89. The subsequent
implementation raised the MSRV to 1.89 and validated tests/Clippy on it; see the
implementation guide's historical validation report. Preserve the lockfile
and keep suitesparse/sundials external backends disabled.

## Acceptance tests for the implementation

- Production sparse/dense interfaces: known nonsymmetric systems, a pivoting
  case, repeated RHS solves, duplicate summation/cancellation, mutation after
  factorization, non-square and RHS mismatches, empty-system policy, non-finite
  input/overflow, structurally and numerically singular systems, residuals.
- DC: stamped resistor divider, ideal V/I sources with correct branch signs,
  disconnected but individually nonsingular blocks, and invalid ideal-source
  loops; compare to the C binary at the roadmap's justified 1e-12 relative
  target with an explicit near-zero absolute scale.
- diffsol: analytic RC and RL responses, an RLC reference, singular mass with
  an algebraic source equation, consistent/inconsistent initial conditions,
  floating-capacitor structure checks, source breakpoints and error paths.
- AC: resistor divider and RC/RLC complex gain and phase.
- Preserve the existing topology regressions and golden/rawfile IO tests.
  Compare transient values on a common sample grid with justified physical
  error bounds, not identical internal timestep sequences.
- Run `cargo test --workspace --locked`, workspace Clippy, and CI on the
  explicitly supported minimum Rust. Replace stub-expectation tests only as
  their corresponding production functionality becomes implemented.

## Sources inspected

Repository paths above; `docs/port/ROADMAP.md` M2–M4;
`src/spicelib/analysis/dctran.c` breakpoint control. Published source for the
locked releases:

- [diffsol builder](https://docs.rs/diffsol/0.17.1/src/diffsol/ode_solver/builder.rs.html)
- [diffsol state/consistent initialization](https://docs.rs/diffsol/0.17.1/src/diffsol/ode_solver/state.rs.html)
- [diffsol BDF state/order](https://docs.rs/diffsol/0.17.1/src/diffsol/ode_solver/bdf_state.rs.html)
- [diffsol API and sparsity documentation](https://docs.rs/diffsol/0.17.1/diffsol/)
- [faer sparse solver API](https://docs.rs/faer/0.24.4/faer/sparse/linalg/solvers/index.html)

The corresponding locally cached registry sources were inspected directly;
this recommendation does not rely only on manifest comments or smoke tests.
