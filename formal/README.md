# Formal-verification pilot (`formal/`)

A Lean 4 + mathlib development that states and proves properties of
ngspice-rs's numerical kernels. It is a *pilot*: it exists to establish what
mechanised proof costs here, where the trust boundary falls, and which targets
are worth proving next. See
[`docs/port/FORMAL_VERIFICATION.md`](../docs/port/FORMAL_VERIFICATION.md) for
the assessment and roadmap, including
[§10](../docs/port/FORMAL_VERIFICATION.md#10-using-the-c-oracle) on how the C
oracle maps into Lean.

This directory is **not** a Cargo package and is not built by
`cargo build`/`cargo test`. It has its own toolchain and its own command.

## Build

```sh
cd formal
lake exe cache get     # ~7 GB of mathlib artifacts, once per machine
lake build             # ~5 s cold for these five modules once mathlib is cached
```

The toolchain is pinned in `formal/lean-toolchain` and every dependency
revision in `formal/lake-manifest.json`; both are committed.
`.lake/` is git-ignored.

To confirm that nothing here is assumed rather than proved:

```sh
cd formal && lake env lean - <<'EOF'
import NgspiceRs
#print axioms NgspiceRs.corrector_exact
#print axioms NgspiceRs.order_two_not_exact
EOF
```

Both print only Lean's three standard axioms (`propext`, `Classical.choice`,
`Quot.sound`). There is no `sorry`, no `admit`, and no `axiom` in this
development.

## What is proved

### `NgspiceRs/Horner.lean` — `src/maths/polynomial.rs`

| Theorem | Statement |
| --- | --- |
| `horner_eq_sum` | The descending fold in `evaluate` computes `Σ_k c_k x^k` for every coefficient list and every point. |

The Rust test for `evaluate` checks one cubic; the identity it depends on holds
for all inputs.

### `NgspiceRs/Integrator/Corrector.lean` — `src/maths/integrator.rs`

`IsCorrector ag s m` is the *definition* of a companion-model corrector: with
`s i = t_trial - t_i` the elapsed times of the charge history, it is the
condition `Σ_i ag i * (s i)^j = -(j == 1)` for `j = 0 .. m-1`. This is exactly
the linear system `nicomcof.c` builds (and that the Rust test helper
`vandermonde_ag` solves independently).

| Theorem | Statement |
| --- | --- |
| `corrector_exact` | If `ag` satisfies the moment conditions up to degree `m-1`, then `Σ_i ag i * P(s i) = -c₁` for every degree-`< m` polynomial `P = Σ_j c j * X^j` — i.e. the corrector differentiates degree-`< m` charge histories exactly. |
| `corrector_exact_self` | The same with the moment set and coefficient set both tied to `n` (`ag` has `n` entries, degree `< n`). |
| `corrector_exact_time` | Restatement in the time coordinate: a charge history sampled from a degree-`< m` polynomial in `t` yields its exact derivative at the trial point, with no residual sign. |

This is the law that `gear_is_exact_for_polynomials_on_nonuniform_steps` and
`high_order_gear_is_exact_for_polynomials_on_nonuniform_steps` sample. The
proof covers *all* coefficient vectors and *all* step sequences at once; it
works for any number of moments, which is what lets `Orders.lean` reason about
one moment more than there are unknowns.

### `NgspiceRs/Orders.lean`

Maximality, for the two orders `dctran.c` can actually select.

| Theorem | Statement |
| --- | --- |
| `order_one_closed_form` | Order 1 forces `ag 0 = 1/h`, `ag 1 = -1/h` — the closed form the code documents but computes by LU. |
| `order_one_not_exact` | On `s ↦ s²` the order-1 rule returns `-h`, not `0`, so its order is exactly 1. |
| `order_two_closed_form` | Order 2 forces `ag 2 = -s₁/(s₂(s₁-s₂))`, `ag 1 = s₂/(s₁(s₁-s₂))`, `ag 0 = (s₁+s₂)/(s₁s₂)`. |
| `order_two_not_exact` | On `s ↦ s³` the order-2 rule returns exactly `s₁ s₂ ≠ 0`, so its order is exactly 2. |

`order_one_error_ne_zero` and `order_two_error_ne_zero` give the same facts in
the "the error is not zero" form that the Rust tests assert numerically.

The general-`n` versions of both statements — the error value is
`(-1)^(n-1) * ∏_{i≠0} s i` — are recorded as a proof sketch at the end of
`Orders.lean`. They are **not** proved here.

### `NgspiceRs/Integrator/Theta.lean` — the trapezoidal/`xmu` rule

A complete instance of the "transcribe a C routine, then prove its law"
pattern described in
[`docs/port/FORMAL_VERIFICATION.md`](../docs/port/FORMAL_VERIFICATION.md#102-what-does-pay-off-iterative-specification-mining):
`trapAg0`/`trapAg1`/`trapStep` mirror `nicomcof.c`'s `case TRAPEZOIDAL` and
`NIintegrate`'s order-2 combination literally, and the theorems are about the
rule.

| Theorem | Statement |
| --- | --- |
| `trap_xmu_zero_is_backward_euler` | `xmu = 0` collapses to `d_n = (q_n - q_{n-1})/h`, which is what `cktntask.c` calls backward Euler. |
| `trap_xmu_half_is_trapezoidal` | `xmu = 1/2` is exactly `2(q_n - q_{n-1})/h - d_{n-1}`, the standard trapezoidal rule. |
| `trapStep_exact_affine` | Affine charge is differentiated exactly, for **every** admissible `xmu`. |
| `trapStep_quadratic` | The error on `t ↦ t²` is exactly `h(2 xmu - 1)/(1 - xmu)`. |
| `trapStep_quadratic_exact_iff` | That error vanishes **iff** `xmu = 1/2`. |
| `trap_xmu_49_is_first_order` | The `0.49` that `cktntask.c` recommends for damping current ringing costs exactly `2h/51` on the quadratic. |
| `trap_xmu_49_error_ne_zero` | The same fact as a failure to reproduce the quadratic, the form the tests assert numerically. |

Consequence: the port accepts `xmu` over all of `[0, 0.5]`, so the
"trapezoidal" method is second-order **only at the default**; every other value
silently demotes it to first order. `cktntask.c`'s comment shows `0.49` is a
deliberate damping trade, not a bug — but the consequence is now a theorem and
no test covers it.

`probes/xmu_order_probe.py` checks the prediction against the C reference: a
linearly ramping current source into a capacitor makes the charge exactly
quadratic, and the measured error falls ~4× per halving of the step at
`xmu = 0.5` against ~2× at `xmu = 0.49`, with a 63.6× same-grid penalty. The
observed slopes are not exactly 2 and 1 because the driver composes the rule
with order-1 startup steps and LTE step control; the theorem is about the rule.
See [§10.5](../docs/port/FORMAL_VERIFICATION.md#105-worked-example-the-xmu-rule).

## Trust boundary

This is the part that matters when reading anything above as a claim about the
shipped simulator.

* **Everything here is over `ℝ`.** The Rust code computes in `f64`. Nothing in
  this development mentions `Float`, IEEE 754, rounding, `mul_add`/FMA, or the
  port's `is_finite` guards. So the theorems say what the *scheme* is, not what
  a particular `f64` run returns.
* **Nothing here references the Rust source.** There is no extraction
  (Aeneas/Charon/hax), no generated model, and no checker that the Rust
  function's body matches `IsCorrector`. The link between the two is currently
  by inspection plus the pre-existing Rust tests.
* Consequences, stated plainly:
  * `corrector_exact` justifies *why* the existing numerical tests pass and
    generalises them beyond the sampled polynomials and step sizes. It does not
    prove anything about `gear_corrector`'s LU loop.
  * `order_one_closed_form` describes the unique value the moment conditions
    force. That the Rust LU *computes* that value is a separate claim, checked
    numerically today and only by extraction tomorrow.
  * A bug in the Rust transcription — a wrong loop bound, a swapped index, a
    different summation order — would not be caught by anything in this
    directory.

Closing that boundary is the entire subject of
[`docs/port/FORMAL_VERIFICATION.md`](../docs/port/FORMAL_VERIFICATION.md) —
which reports the measured result of attempting it: as of `cargo-hax 0.4.2`,
neither `maths/polynomial.rs::evaluate` (closure body) nor
`maths/integrator.rs::gear_corrector` (returns inside nested loops) is
extractable. **No theorem in this directory is currently linked to the Rust
source by machine.**

## Layout

```
formal/
  lean-toolchain              pinned Lean version
  lakefile.toml               mathlib dependency
  lake-manifest.json          pinned revisions (committed)
  NgspiceRs.lean              root module
  NgspiceRs/
    Horner.lean               polynomial evaluation
    Integrator.lean           integrator re-export
    Integrator/Corrector.lean corrector specification and exactness law
    Integrator/Theta.lean     the C trapezoidal/xmu rule, transcribed and proved
    Orders.lean               closed forms and maximal order
  probes/
    xmu_order_probe.py        oracle check for the Theta.lean prediction
```

The probe needs the opt-in C reference and is not part of `lake build`:

```sh
NGSPICE_BIN=/abs/path/to/ngspice python3 formal/probes/xmu_order_probe.py
```

It prints the convergence table and the same-grid penalty; it asserts nothing,
so read its output rather than its exit status.

Files are Lean *modules* (`module` + `public import`), so exported names carry
`public` and definitions whose bodies callers must unfold carry `@[expose]`.
