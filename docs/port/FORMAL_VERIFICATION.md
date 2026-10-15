# Formal verification with Lean 4

An assessment of what it would take to add mechanised proof to ngspice-rs,
written after building a working pilot under `formal/` rather than from
general principles. Read [`formal/README.md`](../../formal/README.md) for the
pilot itself.

**Status:** exploratory. Nothing in `formal/` is proof about the shipped
simulator — see [§3](#3-the-two-gaps) for exactly how far the claims reach.

- [1. Recommendation](#1-recommendation)
- [2. What the pilot established](#2-what-the-pilot-established)
- [3. The two gaps](#3-the-two-gaps)
- [4. Extraction spike: measured results](#4-extraction-spike-measured-results)
- [5. Target tiers](#5-target-tiers)
- [6. Tooling and cost](#6-tooling-and-cost)
- [7. Blockers in this codebase](#7-blockers-in-this-codebase)
- [8. Non-goals](#8-non-goals)
- [9. Suggested first milestones](#9-suggested-first-milestones)
- [10. Using the C oracle](#10-using-the-c-oracle)
- [11. What about c2rust?](#11-what-about-c2rust)
- [12. Gradually moving off C as the oracle](#12-gradually-moving-off-c-as-the-oracle)

---

## 1. Recommendation

**Worth doing, at specification level, for the numerical kernels — and not
worth attempting end-to-end.**

Three separable activities are usually conflated under "formal verification".
They have wildly different costs, and only the first is cheap:

| Activity | Cost | What it buys |
| --- | --- | --- |
| **Specification proofs** over `ℝ`: state what each kernel must compute and prove the scheme has that property, for all inputs and all step sequences. | Low. The pilot did the integrator in an afternoon. | Turns sampled numerical tests into theorems; explains *why* the C-parity results hold; pins down anti-properties (e.g. "order 2 is *exactly* order 2") that tests can only sample. Does not touch the Rust source. |
| **Refinement proofs** over `f64`: prove the Rust function body computes the spec, including rounding. | **Blocked, not merely expensive.** [§4](#4-extraction-spike-measured-results) measured that the only Lean-side pipeline has no floating-point model at all, so no `f64` kernel is translatable at this version. | What most people mean by "verified simulator". |
| **Structural proofs** over the combinatorics: MNA ground elimination, branch ordering, pattern/pivot-order determinism. Float-free, so both cheap *and* genuinely about the code. | Medium, gated on being able to model the device layer. | Catches a real class of bugs (index spaces, sign conventions, ordering) that testing catches only by accident. |

The recommendation is to fund the first and third, treat the second as
blocked upstream rather than as an engineering project, and be explicit in any
public claim that specification proofs are specification proofs. The existing C-differential test suite is the right
instrument for the second tier and already does most of the available work;
proof should be aimed at what testing structurally cannot reach.

## 2. What the pilot established

Concrete, reproducible facts from building `formal/`:

| Observation | Measurement |
| --- | --- |
| Lean toolchain | Already installed on this machine: Lean 4.35.0 + Lake 5.0.0 via `elan`. No new toolchain needed. |
| mathlib dependency | `master` @ `6015dbb`, 9 039 cached oleans, **7.2 GB** of build artifacts. One-off `lake exe cache get` took several minutes on a fast connection. |
| Incremental build of the four project modules | **~5 s**, and a single file is ~1.5 s. Editing proofs is interactive-speed. |
| Proofs written | 22 public declarations across 4 modules; zero `sorry`, zero `axiom`. `#print axioms` reports only `propext`, `Classical.choice`, `Quot.sound`. |
| Oracle check | One proved rule was checked against the C binary: the predicted order separation in `.option xmu` reproduces, with a 63.6× same-grid error penalty at `xmu = 0.49` ([§10.5](#105-worked-example-the-xmu-rule)). |
| Effort | Roughly one session, most of it spent on mathlib API archaeology and on the sign conventions (see below), not on mathematics. |

Two findings are worth recording because they cost real time and will cost it
again:

- **The `-1` in the moment conditions is the whole subtlety.** The Gear
  coefficients are built from *elapsed* times `s i = t_trial - t_i`, so
  `d/ds = -d/dt` and the moment system has right-hand side `-1` at `j = 1`,
  not `+1`. Getting this wrong produces a formalisation that looks right and
  proves a false statement about the code. It is the reason
  `corrector_exact` returns `-c₁` while `corrector_exact_time` returns `+b₁`.
  Any future reviewer should check this sign first.
- **Lean's module system is now the default for mathlib and is not optional in
  practice.** Files want `module` + `public import`; declarations need
  `public`; and any definition whose *body* a downstream file must unfold needs
  `@[expose]`. Getting this wrong yields a confusing "unknown identifier" or
  "function expected" error rather than a module-system diagnostic. Budget an
  hour for this, or set the project up from a known-good template.

## 3. The two gaps

### 3.1 The float gap

Everything in `formal/` is stated over `ℝ`. The code computes in
`f64` (`src/primitives/value.rs:37`, `pub type Real = f64`).

That is not laziness; it is the honest boundary. IEEE 754 `f64` arithmetic is
not a ring — addition is not associative, distributivity fails — so
`corrector_exact`'s proof, which swaps two finite sums and then uses `ring`,
does not transfer. Three ways to bridge it, none cheap:

1. **Don't.** State the scheme's mathematics over `ℝ`, and keep `f64` behaviour
   in the C-differential suite. This is what the pilot does, and it is
   defensible: the theorems are about the *algorithm*, and the tests are about
   the *implementation*.
2. **Prove over the logical model of `Float`.** Lean has a proof model of
   `Float`/`Float32` with canonicalised NaN, and Mathlib carries experimental
   IEEE support (`Mathlib.Data.FP`); standalone projects exist too
   (`FloatSpec`, a port of Coq's Flocq). Proofs then hold for all inputs, but
   every `ring`-style step becomes a rounding-error argument. Realistically
   this multiplies proof effort by an order of magnitude and produces bounds —
   "within `k·ulp`" — rather than equalities.
3. **Prove over `ℝ` and separately bound the error.** The classical numerical-
   analysis route. Requires forward-error analysis per kernel. Also expensive,
   and the result is only as good as the bound.

The gap is worse than generic for this codebase in one specific way:
`fit_power_basis` (`src/maths/polynomial.rs`) calls `f64::mul_add`, which
compiles to a hardware FMA where available and to a fused-software sequence
otherwise. `VERIFICATION.md` records that the port matches the *no-FMA* C
build bit-for-bit and differs from the default C build. A refined model of
`fit_power_basis` would therefore have to be parameterised on whether FMA
exists on the target — i.e. the property being proved is platform-dependent.
Every `mul_add` in the tree is in that one file; elsewhere the arithmetic is
ordinary `+`/`*`/`/`, which at least does not add a platform axis.

### 3.2 The refinement gap

Nothing in `formal/` refers to the Rust source. There is no extraction, no
generated model, no body-equivalence check. So the development currently proves
*properties of a specification that was transcribed by hand from the code and
its comments*.

Concretely: `order_one_closed_form` proves that the moment conditions force
`ag 0 = 1/h`. It does not prove that `gear_corrector`'s pivoting-free LU loop
produces that value. A wrong loop bound, a swapped index or a different
summation order in `src/maths/integrator.rs` would not be caught.

Closing this requires one of:

- **Extraction (Aeneas/Charon, driven through `cargo hax into lean`).** Charon
  is an rustc driver that lowers MIR to LLBC; Aeneas turns LLBC into a pure
  recursive Lean model. As of hax 0.4 (Sept 2026) the Lean backend is the
  Charon+Aeneas pipeline, with prebuilt binaries fetched on demand. This is the
  only mainstream route to a *machine-checked* link, and it is the route to
  budget for. Costs and blockers are in [§6](#6-tooling-and-cost) and
  [§7](#7-blockers-in-this-codebase).
- **An executable oracle.** Have Lean compute the proved closed form and have a
  Rust test compare the kernel's output against it — the same comparison
  `vandermonde_ag` already performs, but against an oracle *derived from the
  proof*. This is not a proof of refinement — it is testing with a
  better-specified oracle — but it is cheap and it does catch
  transcription errors that the hand-transcribed spec would let through.
- **Soundness by review.** What exists now. Worth stating as a deliberate
  choice rather than leaving it implicit.

## 4. Extraction spike: measured results

This is the highest-information experiment available, so it was run rather than
estimated. Everything below is observed behaviour on this tree, not
extrapolation.

### Setup

```
cargo-hax        0.4.2
charon           nightly-2026.09.02   (downloaded on demand by hax)
aeneas           build-183e4f0         (downloaded on demand by hax, via hax-lean-lib v0.3.30)
generated Lean   leanprover/lean4:v4.31.0
```

Note the generated project pins **Lean v4.31.0**, four minor versions behind
the `v4.35.0` the pilot uses, and mathlib is not involved. An extracted project
and `formal/` cannot share one toolchain or one `lake` workspace.

Timings: a default whole-target run is dominated by the Charon-driven `cargo
build` and took **~4 min 20 s**. A `--start-from` run completed in **~30 s**,
which makes iterating on individual functions cheap once the target is chosen.

### What happened

| Attempt | Result |
| --- | --- |
| Default invocation (`cargo hax into --output-dir … lean`) | hax translates the **binary** target. Aeneas aborts on `spice_rs::main` with `Assertion failed: new value doesn't have the same type as its destination` at `src/bin/spice-rs.rs:20:28-20:35`, writes empty `Funs.lean`/`Types.lean`, and exits 1. No library function is extracted. |
| `-i '+ngspice_rs::**::evaluate'`, with and without `--lib` passed via `-C` | **No effect in these runs.** The binary target was still processed and library items still did not appear. The item-selection flag documented for `into` did not reach the Charon pipeline; Charon's own `--start-from`/`--include` do. |
| `--charon-args="--start-from 'ngspice_rs::maths::polynomial::evaluate'"` | Charon succeeds and emits `pub fn evaluate(_1: &[f64], _2: f64) -> f64` — **but as `<opaque>`, with no body.** Aeneas therefore generates empty modules. `--start-from` alone is not enough; the body must also be made non-opaque. |
| Same, plus `--include 'ngspice_rs::maths::polynomial::evaluate'` | Charon now produces a 408 KB LLBC containing the real MIR, including the closure `evaluate::closure` and its `call_mut` body. Aeneas then fails: `Invalid inputs for binop` at **`src/maths/polynomial.rs:66:48-66:55`** — i.e. the `sum * x` inside `\|sum, c\| sum * x + c` — and warns that `DoubleEndedIterator`/`Iterator` are mutually recursive trait declarations, which it does not support. (The first version of this section read that as "the closure is the problem". It is not — see the correction below.) |
| `--start-from`/`--include` on `ngspice_rs::maths::integrator::gear_corrector` | Aeneas fails with `Returns inside of nested loops are not supported yet` at `src/maths/integrator.rs:393:0-397:1`, then ignores the body. |

### What it means

**Correction.** The first version of this section attributed `evaluate`'s
failure to its closure. That was wrong. The isolating probe in
`formal/probes/extraction_shape_probe.sh` shows that iterator-and-closure shapes
extract fine when the element type is an integer, and that everything containing
an `f64` fails. The cause is the float type, not the closure. The
misattribution is left on the record because it is instructive: the diagnostic
`Invalid inputs for binop` names the operand types of an arithmetic operation,
and reading it as "the closure" is exactly the inference that a probe settles
and prose does not.

The probe, run with the same toolchain versions and with each function
translated in isolation:

| probe | extracted | first diagnostic |
| --- | --- | --- |
| `f64_literal` — a bare `1.5` | **no** | `Improperly typed constant value` |
| `f64_binop` — `a * b` on two `f64` parameters | **no** | `Invalid inputs for binop` |
| `f64_loop` — `f64` accumulation in a `while` loop | **no** | `Improperly typed constant value` |
| `int_binop` — `a * b` on `usize` | yes | — |
| `int_closure_fold` — `iter().rev().fold(…, \|s, c\| s + c)` on `usize` | yes | — |
| `int_nested_early_return` — nested loops with an early return | **no** | `Returns inside of nested loops are not supported yet` |
| `raw_ptr_i32_loop` — `*a.add(i)` over `*const i32` | **no** | `Aeneas does not yet support dereferencing raw pointers.` |
| `raw_ptr_f64_loop` — the same over `*const f64` | **no** | `Improperly typed constant value` |

So there are three independent limitations, not one:

- **No floating-point model.** Aeneas's Lean backend defines no `F32`, `F64` or
  `Float` anywhere under `Aeneas/Std`; its scalar model is integers and
  bitvectors. A bare float *literal* therefore fails, not just arithmetic. Float
  support for the Lean backend is an open, unmerged upstream draft (Aeneas
  PR #1073, whose description notes that "Lean's own support for floats does not
  seem super advanced, so lots of core routines are `opaque`"), and float
  literals alone needed a dedicated hax PR (#880).
- **No raw-pointer dereference.** This is the c2rust memory model, and it is the
  one diagnostic that names its own limitation outright. See
  [§11](#11-what-about-c2rust).
- **No returns inside nested loops.** Genuine, float-independent, and what stops
  `gear_corrector`.

**This changes the conclusion, and it is worse than the original section said.**
For a SPICE simulator, "no floats" is not a subset restriction — it is the whole
language. Every kernel in `src/maths` and every device stamp is `f64`
arithmetic. Tier 1 is therefore not gated on the shape of the code, on the size
of a closure, or on refactoring effort: it is gated on an upstream float model
that does not exist in this build. Two gates have to open, and this one is
first.

That also withdraws the mitigation the first version proposed. Rewriting a
kernel closure-free, single-loop and early-return-free — the refactor that
[§10.4](#104-the-oracles-highest-value-role-making-the-refactor-possible) argued
the C oracle could de-risk — would produce code that still fails on its first
`f64`. That refactor is not worth starting until a float model lands.

What survives:

1. **Tier 1 should not be planned at all right now.** Track Aeneas PR #1073 and
   re-run `formal/probes/extraction_shape_probe.sh` after any hax release. Until
   the three `f64_*` rows turn green, no preparatory work in this repository
   moves Tier 1.
2. **Tiers 0 and 2 and the oracle work are unaffected**, because none of them
   depends on extraction. [§10](#10-using-the-c-oracle) is where the value
   available today is.
3. **`gear_corrector` may still be blocked after floats land**, because of the
   nested-loop-return limit. Both gates have to open, and they are independent.

Two incidental findings worth recording, because both are silent-failure
hazards for any scripted extraction:

- **`--start-from` without `--include` silently yields `<opaque>`.** The run
  "succeeds" — exit 0, files written — while containing nothing.
- **Aeneas emits a `def` with body `sorry` for functions it failed to
  translate.** So "the name appears in `Funs.lean`" is not evidence of success;
  the body has to be checked for `sorry`. The probe script does this, and
  asserts on the body rather than on the name.

## 5. Target tiers

Ordered by benefit per unit of effort, given what the pilot showed. "Float-free"
means the property is about logic/indexing/ordering and needs no real analysis
to state.

### Tier 0 — specification laws for numeric kernels (float-free statements over `ℝ`)

Cheap, immediate, already partly done.

| Target | Location | Property |
| --- | --- | --- |
| Corrector exactness | `maths/integrator.rs` | **Done** (`corrector_exact`, `corrector_exact_time`). |
| Order-1/2 closed forms and maximality | `maths/integrator.rs` | **Done** (`order_*`). General-`n` maximality (`(-1)^(n-1)·∏ sᵢ`) is sketched, not proved. |
| Horner | `maths/polynomial.rs` | **Done** (`horner_eq_sum`). |
| Truncation timestep `CKTterr` | `maths/integrator.rs` | `truncation_timestep_follows_cktterr` and `high_order_truncation_timestep_follows_cktterr` assert an exact formula for the `(order+1)`-th divided difference of a polynomial; a theorem would remove the division the tests do on a single polynomial. |
| Interpolation | `maths/polynomial.rs` | `fit_power_basis` claims the unique interpolant through distinct samples. The theorem is: distinct `x` ⇒ the returned coefficients satisfy `evaluate c (x[i]) = y[i]`, and are unique. Currently a test asserts one quartic plus two degenerate cases. |
| Polynomial evaluation and root search | `maths/polynomial.rs`, pole-zero path | Algebraic identities of the ported root search. |

Value: moderate. This is the tier where a proof generalises an existing passing
test rather than finding anything new. Its real contribution is turning a
"tested at three step sizes" claim into a "for every admissible step sequence"
claim, which matters because transient step sequences are chosen adaptively and
are therefore not enumerable.

### Tier 1 — refinement for the small, float-light kernels

The natural second step, and the only realistic way to make Tier 0 statements
about the Rust code. Candidate kernels must have a *small transitive closure*:
Aeneas translates whole functions, so the function plus everything it calls
must be in the translatable subset.

Good first candidates, by closure size:

- `maths/polynomial.rs::evaluate` — takes `&[f64]`, returns `f64`. Closure is
  `iter().rev().fold`. Should translate trivially; gives an end-to-end
  mechanically-checked `horner_eq_sum` *about the Rust function*.
- `maths/polynomial.rs::fit_power_basis` — `Vec<Vec<f64>>`, `mul_add`, `abs`,
  `is_finite`. Larger, and the FMA question in [§3.1](#31-the-float-gap) has to
  be answered.
- `primitives/complex.rs` — complex arithmetic; likely translatable, and
  `mul_add`-free.
- `maths/integrator.rs::gear_corrector` — the prize: it would upgrade
  `order_one_closed_form` from "the moments force this value" to "this function
  computes this value". It is a fixed-size array computation with
  `#[allow(clippy::needless_range_loop)]` loops, no allocation, no traits — on
  paper an ideal Aeneas target. Verify before committing to it.

This tier is where the real work is, and it should be attempted on **one**
function as a spike before any wider commitment, because its outcome is
currently unknown and everything downstream depends on it.

### Tier 2 — structural invariants

Float-free, and therefore the tier where proof is both cheap and about the
code. The problem is that these properties live in `src/devices` and
`src/analysis`, which are the hardest parts of the tree to model
([§7](#7-blockers-in-this-codebase)).

| Target | Property |
| --- | --- |
| `primitives/node.rs` | Node ids are dense and stable; id 0 is ground; `NodeKind::Ground` is the only non-unknown; `gnd` aliasing respects `no_auto_gnd`. |
| MNA assembly | Ground is eliminated and row 0 is an ordinary unknown; node rows are allocated before branch-current rows; each device's branch range is contiguous and preserved. AGENTS.md states all of these as invariants nothing currently enforces. |
| Branch sign convention | Source/inductor current is positive from the first terminal to the second, and the plot name matches. |
| `maths/sparse13.rs` | One pivot order per circuit pattern; refactoring with it; reorder-on-zero-pivot. |

The highest-value item here is the MNA row/unknown allocation, because the
AGENTS.md guardrails call out that circuit node ids, device ordinals, graph
handles and matrix rows are four different namespaces, and a proof is the only
way to make "different namespaces" more than a comment.

### Tier 3 — sparse LU correctness

The prize and the trap. The property is `P A = L U` (or `A = L U` for the
SPARSE variant), plus `solve` inverts the factorization, plus agreement with
C's pivot sequence. Obstacles:

- The implementation is 1 790 lines of linked-list arena manipulation that
  deliberately mirrors C's control flow, including element creation order and
  tie-breaking, so that pivot sequences match SPARSE. Proving *that* requires
  modelling the element arena, the `Slot` "address of a link" abstraction, and
  `expandable` storage.
- `f64` throughout, so Tier 1's refinement problem applies in full.
- A tractable intermediate: prove the *general* `PA = LU` property for a dense
  reference implementation over `ℝ` in the same pivot-order discipline, and
  test the sparse path against it. That is a meaningful but partial result, and
  it is the honest shape of "verifying the solver".

Recommendation: treat Tier 3 as a separate project with its own go/no-go, not
as a continuation of Tiers 0–2.

## 6. Tooling and cost

### 6.1 Toolchains

- Lean is **not** currently a project dependency and should not become one.
  `formal/` sits outside the Cargo workspace (`[workspace] members = ["xtask"]`),
  is excluded from `Cargo.toml`'s `include` list so it never reaches the
  published crate, and is built by `lake` rather than `cargo`.
- Pin `lean-toolchain` **and** commit `lake-manifest.json`, as the pilot does.
  mathlib `master` moves; an unpinned dependency turns a green build red
  without a source change.
- The MSRV story is unaffected: this is a separate toolchain for a separate
  build, exactly as the optional upstream C checkout is.

### 6.2 CI

A Lean job is cheap *if* the mathlib cache is handled properly:

```yaml
- run: cd formal && lake exe cache get && lake build
```

Cost profile: ~7 GB of artifacts to fetch or restore. Without an
`actions/cache` entry for `formal/.lake` (or mathlib's cache directory) that is
a multi-minute download per run; with one, the build itself is seconds. A
`push`/`pull_request` job is therefore viable, but caching is not optional —
an uncached Lean job is slower than the entire Rust test suite.

Suggested gating: run `lake build` on every PR and treat it as required, since
a broken proof is a build failure and the build is fast. Do **not** make the
Lean job a release gate in the same job as `cargo package`; keep them
independent so a mathlib upstream break cannot block a Rust release.

`lake build` is the whole gate. There is no `lake fmt`/`lake clippy`
equivalent worth wiring up; the linter warnings that matter (unused `simp`
arguments, unused variables) are on by default and the pilot's build is
warning-free.

### 6.3 The extraction toolchain, if Tier 1 is pursued

`cargo hax into lean` (hax 0.4.2) orchestrates Charon + Aeneas, downloading
pinned prebuilt binaries on demand. [§4](#4-extraction-spike-measured-results)
records what that produced on this tree. The costs to plan for:

- **Charon is a rustc driver.** hax pins `charon
ightly-2026.09.02` and the
  project pins `stable` with MSRV 1.89. Extraction therefore cannot share the
  project's toolchain: you need a second, independently pinned toolchain in
  CI, and it will stop working whenever its pinned nightly is withdrawn. This
  is an ongoing maintenance tax, not a one-off setup cost.
- **The generated Lean project pins its own Lean version** (`v4.31.0` for this
  hax release, versus the pilot's `v4.35.0`) and does not use mathlib. Extracted
  code and hand-written specifications cannot live in one `lake` workspace until
  those versions are reconciled, which means a third pinned toolchain or an
  upgrade of one side to match the other.
- **Charon's output is beta.** It reports unsupported Rust features (tracked
  upstream), does not support trait solving on its output, and has planned
  breaking changes in name matching and representation. Expect the extraction
  configuration to need revision on upgrades, and treat any extracted model as
  needing re-derivation rather than as a stable artifact.
- **Aeneas's supported subset is the binding constraint**, and
  [§4](#4-extraction-spike-measured-results) shows this tree currently falls
  outside it for two independent reasons (closures, and returns/nesting inside
  loops). Treat "what does Aeneas support this quarter" as the project's real
  dependency, and re-probe after every hax release.

### 6.4 The mathlib download is the only cost that is already paid

7.2 GB is a lot to put in CI. Two options: cache `formal/.lake` (or the
mathlib cache directory) between runs, or run the Lean job only when files
under `formal/` change. Both are normal; neither is a reason to avoid the work.

## 7. Blockers in this codebase

The Rust tree is 146 k lines (90 k under `src/`), with 195 public functions in
`maths` + `primitives` alone. Features that matter for translation, measured on
the current tree:

| Feature | Count / location | Impact |
| --- | --- | --- |
| `unsafe` | **0** (two mentions, both comments) | Favourable. `unsafe_code = "forbid"` removes the hardest extraction case outright. |
| `dyn Device` | 16 sites in `devices/circuit.rs`, plus every device module | The main structural blocker. `Circuit` holds `Vec<Box<dyn Device>>`; a model of "the circuit" must model dynamic dispatch over ~20 device types. This is what makes Tier 2 expensive. |
| `BTreeMap` / `HashMap` | 32 in `devices/subckt.rs`, 16 in `netlist/eval.rs`, plus most of `devices/` and `analysis/` | No representation in the translated model; would need a Lean model of ordered maps, or a refactor to slices for the parts being verified. |
| `Arc`, `Rc`, `RefCell`, `Cell` | `maths/sparse.rs` (`Arc`, `Cell`, `HashSet`), `maths/linear.rs` (`Arc`), `devices/circuit.rs` (`RefCell`), `devices/nonlinear.rs` (`Rc`) | Interior mutability and shared ownership do not fit the pure-functional Aeneas model. Concentrated in exactly the modules one would want to verify. |
| Iterator chains / closures | ~1 374 iterator-chain calls, ~143 closure literals | Iterators and closures are supported to a degree, but the *volume* is the problem: these are pervasive inside `maths/sparse13.rs` and would dominate any modelling effort. |
| `mul_add` (FMA) | 5, all in `maths/polynomial.rs` | Makes even a successful refinement platform-dependent (see [§3.1](#31-the-float-gap)). |

Two conclusions:

1. **The translatable subset is much smaller than `src/`.** The functions that
   stand a chance are small, allocation-light and free of traits, shared
   ownership and interior mutability. That is the opposite of where the
   interesting simulation logic lives. Plan for Tier 1 to cover a handful of
   kernels, not modules.
2. **`maths/sparse13.rs` is the interesting case and the worst case.** It is
   `Arc`/`Cell`/`HashSet`-bearing, iterator-heavy, and deliberately a
   line-by-line mirror of C's linked-list control flow — and it is also the
   component whose correctness matters most. If verification effort is ever
   concentrated anywhere, it should be here, and it should be expected to
   require either a hand-written model plus a refinement argument, or a
   deliberate refactor of the port toward a pure-functional core with a thin
   imperative shell that mirrors C.

## 8. Non-goals

Explicitly out of scope, so that a bounded result is not read as a larger one:

- Proving the simulator's output equals ngspice's. That is C-parity, and the
  golden/conformance suite plus the opt-in `c_*_reference` tests are the right
  instruments.
- Proving anything about convergence policy, iteration limits, step control or
  error recovery. These are policy and heuristic, not theorems; `dctran.c`'s
  control flow is the specification, and the port's job is to mirror it.
- Proving the device models (MOS levels, BJT, JFET, LTRA, behavioural) correct
  against semiconductor physics. The specification here is "what ngspice
  computes", which is a transcription-fidelity question, not a physics one.
- Proving the parser, the rawfile writer or the CLI. Text handling and I/O are
  the worst possible proof targets, and the snapshot tests already pin them.
- Replacing the test suite. Proof and the C-oracle tests are complementary:
  proof covers the universal algebraic laws and the anti-properties; the oracle
  covers `f64` behaviour on real decks.

## 9. Suggested first milestones

Bounded, in order, each of which produces something checkable.

1. **Land the pilot as a separate, non-blocking build.** `formal/` as it stands:
   22 declarations, no `sorry`, `#print axioms` clean, warning-free, with a CI
   job that caches mathlib. Add the `.gitignore` entry (done). Treat it as
   documentation of the numerical kernels, not as a publication claim.
   *Cost: review only.*
2. **Do not start Tier 1 work; track the upstream gate instead.** The probe is
   done and the answer is no: [§4](#4-extraction-spike-measured-results) shows
   Aeneas's Lean backend has no `f64` model, so Tier 1 is blocked on upstream
   PR #1073 rather than on anything here. The action is to re-run
   `formal/probes/extraction_shape_probe.sh` after any hax release and start
   Tier 1 only when its `f64_*` rows turn green. *Cost: minutes per release.*
3. **When the float gate opens, extract exactly one kernel** — the smallest one
   with a Tier 0 theorem already attached, so the proof obligation is "this body
   equals the model I already specified" rather than new mathematics. Given
   `horner_eq_sum`, `evaluate` is the obvious choice. *Cost: days, not weeks,
   once the gate opens.*
4. **Prove the MNA allocation invariants** (Tier 2) as specifications over an
   abstract device interface, and check them against the Rust construction
   logic by inspection plus targeted tests. This avoids `dyn Device` by
   modelling the *interface*, not the concrete devices — the cheapest way to get
   value from the structural tier, and it does not depend on extraction. *Cost:
   weeks.*
5. **Extend Tier 0.** The general-`n` maximality result (`(-1)^(n-1)·∏_{i≠0} s i`,
   sketched in `Orders.lean`), `CKTterr`'s truncation formula, and
   `fit_power_basis`'s interpolation spec are all self-contained and were
   confirmed to be in scope by the pilot. *Cost: days each.*
6. **Decide on sparse LU separately.** Write up the `PA = LU` property for a
   dense reference over `ℝ` first, and only then judge whether the sparse path
   is approachable. *Cost: a project, with its own go/no-go.*

The revised emphasis, relative to a pre-spike sketch, is that the *value* of
this effort no longer hinges on the Rust↔Lean link, because the link is
currently blocked by Aeneas's supported subset rather than by anything under
this project's control. Tiers 0 and 2 are worth doing on their own terms;
Tiers 1 and 3 wait on the toolchain.


## 10. Using the C oracle

The optional upstream C checkout is this project's oracle: goldens captured
from it, and opt-in `c_*_reference` tests that compare a fresh run. The
question "can the oracle be iteratively converted into Lean verifications to
use against Rust" has a good answer, but not the obvious one.

**Short answer.** Yes — but the productive reading is *mine the oracle for
specifications*, not *verify or extract the C*. And the oracle's single most
valuable role in a Lean programme is not as a specification source at all: it is
the safety net that makes the extractability refactor of
[§4](#4-extraction-spike-measured-results) possible ([§10.4](#104-the-oracles-highest-value-role-making-the-refactor-possible)).

### 10.1 The readings that do not pay off

**Verify the C.** CN/Cerberus, Frama-C, VST and CompCert are mature, and they
are Rocq/Coq projects rather than Lean ones. More importantly they are
orthogonal to the goal: for a port, the specification *is* C's behaviour, so
proving the C correct proves the reference equals itself and says nothing about
Rust. This is the one setting where verifying the reference is definitionally
without content.

**Extract the C into Lean.** Two sub-routes, both closed:

- `c2rust` → Charon → Aeneas. c2rust emits unsafe, pointer-heavy Rust, and
  Aeneas's whole method is eliminating memory reasoning *"as long as they do not
  rely on interior mutability or unsafe code"*. The translation destroys the
  precondition for the verification.
- A direct C→Lean 4 exporter. A code-property-graph-based line of work now
  exists that translates C into a small Lean 4 core semantics, and it is
  explicit that its output needs a "trust ledger" precisely because it cannot
  distinguish code it *declined* to translate from code it translated
  *incorrectly*. That is the same failure class as the `<opaque>`-and-exit-0
  trap recorded in [§4](#4-extraction-spike-measured-results), and ngspice's
  scale is far outside what such an exporter targets. It would also land in the
  same Aeneas-subset wall on the Rust side, so it cannot deliver the link that
  matters.

**Treat oracle outputs as axioms.** "The golden says 0.0123 V, therefore Rust
must produce 0.0123 V to within tolerance" is a per-instance fact about one
deck. As a Lean axiom it is unsound in general, and in review it is
indistinguishable from an assumption that happens to agree with a test.

There is an asymmetry here worth stating plainly, because it is a consequence
of choosing Lean: **for C, the mature verification tooling is Rocq; for Rust,
it is Lean.** A Lean project will always have a *tested* C oracle and can never
have a *proved* one. That is a perfectly good arrangement — C's role here is
transcription fidelity, not physics — but it should be a decision rather than a
discovery.

### 10.2 What does pay off: iterative specification mining

Per kernel, four steps. The pilot has already done this by hand once: `IsCorrector`
is `nicomcof.c`'s linear system, transcribed, and `Orders.lean` is the
closed-form side of the same routine.

1. **Transcribe** the C routine into a Lean definition, keeping C's algebra and
   its operation order — so the two can be compared numerically, not only
   mathematically.
2. **Prove** the law the routine exists to implement.
3. **Identify** the definition with a mathematical specification over `ℝ` where
   that is possible; the proved facts then transfer to any faithful
   implementation.
4. **Check** the Lean definition against the C binary by evaluation.

The chain is:

    C  ↔(tested)↔  Lean reference  ↔(proved)↔  math spec  ↔(tested)↔  Rust

Two of the three links are *tested*, so this **upgrades the oracle; it does not
verify the Rust**. Stated honestly: the value is that trust moves from "a number
in a golden file" to "a proved law plus a numeric agreement", and the residual
risk is localised on the Rust↔reference interface — which is exactly where the
existing conformance suite already aims.

Cost is favourable because the C is small per kernel and right there. Step 1 is
mechanical; step 2 is where the pilot spent its time, almost entirely on sign
conventions rather than mathematics.

### 10.3 Also worth doing: verify the comparison itself

`xtask/src/compare.rs` holds the tolerance policy (`DC`, `AC`, `NONLINEAR`,
`NOISE`, `SENSITIVITY`, `TRAN`, …) and is the code that decides what "Rust
matches C" *means*. It is unverified.

Its content is float-light and small: a tolerance relation that should be
symmetric and monotone in the tolerances, a worst-case aggregation, and matching
by variable name across two different value orderings. Proving the algebra —
symmetry; monotonicity; that the reported "worst fraction of bound" really is
the maximum; that name matching cannot silently skip a variable that only one
side has — is cheap and protects every conformance claim the project makes.
This is not the simulator, but it is the machinery that produces the project's
evidence, and a bug in it invalidates whole rows of `VERIFICATION.md`.

### 10.4 The oracle's highest-value role: making the refactor possible

[§4](#4-extraction-spike-measured-results) established that the binding
constraint on refining Rust against Lean is Aeneas's supported Rust subset:
no closure bodies, no returns inside nested loops. Entering that subset means
*reshaping numerical kernels* — and reshaping numerics is precisely the
operation that silently changes answers.

The oracle is what makes that safe, because the goldens are bit-exact against
the no-FMA C build. **This route is currently suspended**, because
[§4](#4-extraction-spike-measured-results) later established that the refactor
would not buy translatability while Aeneas's Lean backend lacks an `f64` model:
step 2 below would produce code that still fails on its first `f64`. The
reasoning is kept because it becomes right again the moment a float model lands,
and because the oracle's role is the same either way. When that happens, the
iteration is:

1. pick a kernel that already has a bit-exact golden;
2. rewrite it closure-free, single-loop, no early return, re-checking
   bit-exactness against that golden at every step;
3. extract the rewritten kernel with hax/Charon/Aeneas;
4. prove the extracted Lean model equals the §10.2 reference;
5. the kernel is now C-tested *and* Lean-proved.

This is the only route on the table that reaches a real refinement result, and
the C oracle is load-bearing in it — not as a specification, but as the
regression net that lets you touch the numerics at all.

**Closed for `sparse13.rs`.** AGENTS.md requires the sparse LU to mirror C's
loop structure and element creation order so that pivot sequences match SPARSE.
Rewriting it for extractability would trade a *required* property for a
*proved* one, which is a bad trade. That file stays on goldens, and the honest
statement is that the solver's correctness rests on C-parity testing.

### 10.5 Worked example: the `xmu` rule

`formal/NgspiceRs/Integrator/Theta.lean` is §10.2 applied to one kernel, and it
turned up a consequence nothing in the test suite pins.

`cktntask.c` documents the trapezoidal rule's parameter and defaults it to
`0.5`, and `cktsopt.c` accepts the whole `[0, 0.5]`:

```c
/*   xmu=0:    Backward Euler
 *   xmu=0.5:  trapezoidal (standard)
 *   xmu=0.49: good damping of current ringing, e.g. in R.O.s. */
tsk->TSKxmu = 0.5;
```

The port mirrors both the range and the comment (`companion.rs`, `.option xmu`).
Proved, for `d_n = -(xmu/(1-xmu)) d_{n-1} + (1/(h(1-xmu))) (q_n - q_{n-1})`:

| Theorem | Statement |
| --- | --- |
| `trap_xmu_zero_is_backward_euler` | `xmu = 0` collapses to `d_n = (q_n - q_{n-1})/h` — C's own first row. |
| `trap_xmu_half_is_trapezoidal` | `xmu = 1/2` is exactly `2(q_n - q_{n-1})/h - d_{n-1}` — C's second row. |
| `trapStep_exact_affine` | Affine charge is differentiated exactly, for **every** admissible `xmu`. |
| `trapStep_quadratic` | The error on `t ↦ t²` is exactly `h(2 xmu - 1)/(1 - xmu)`. |
| `trapStep_quadratic_exact_iff` | That error is zero **iff** `xmu = 1/2`. |
| `trap_xmu_49_is_first_order` | The `0.49` that C recommends for damping costs exactly `2h/51` on the quadratic. |

So the "trapezoidal" method is second-order **only at the default**. Every other
value the port accepts silently demotes it to first order. C's comment shows
`0.49` is a deliberate damping trade rather than a bug — but the consequence is
now a theorem instead of an inference, and no test covers it.

**Oracle confirmation.** `formal/probes/xmu_order_probe.py` drives the C
reference on a circuit whose *charge* is exactly quadratic — a linearly ramping
current source into a capacitor, so `v(1 us) = 1.5e-3 V` exactly and any error
is integration error. The theorem predicts the order; the binary shows it:

| `xmu` | `tmax` | points | `v(1 us)` | rel. error | error ratio |
| --- | --- | --- | --- | --- | --- |
| 0.5 | 0.1 µs | 24 | 1.500013117200e-03 | 8.74e-06 | — |
| 0.5 | 0.05 µs | 33 | 1.500003286800e-03 | 2.19e-06 | 3.99 |
| 0.5 | 0.025 µs | 52 | 1.500000829200e-03 | 5.53e-07 | 3.96 |
| 0.5 | 0.0125 µs | 91 | 1.500000214800e-03 | 1.43e-07 | 3.86 |
| 0.49 | 0.1 µs | 24 | 1.500834598752e-03 | 5.56e-04 | — |
| 0.49 | 0.05 µs | 33 | 1.500458657088e-03 | 3.06e-04 | 1.82 |
| 0.49 | 0.025 µs | 52 | 1.500239671672e-03 | 1.60e-04 | 1.91 |
| 0.49 | 0.0125 µs | 91 | 1.500122425318e-03 | 8.16e-05 | 1.96 |

On the identical 24-point grid, `xmu = 0.49` is **63.6× worse**. The refinement
ratios separate the two rules by exactly one order: ~4× per halving for
`xmu = 0.5` (second order) against ~2× for `xmu = 0.49` (first order). The
observed log-log slopes are 2.38–2.95 and 1.18–1.81 rather than 2.00 and 1.00,
because ngspice's driver composes the rule with order-1 startup steps and
LTE-based step control, and because `tmax` does not fully pin the grid (the
point counts are not doubling). The theorem is about the *rule*; the experiment
confirms the rule's order separation, not the composite order or the magnitude.

That is the pattern working in both directions: **the theorem makes a
falsifiable prediction, and the oracle is the instrument that checks it.** This
is also the honest shape of "using the C oracle against Rust" — it validates
the specification, and the Rust side still needs [§4](#4-extraction-spike-measured-results)
or testing.

### 10.6 Recommended order of work

If the oracle is the asset, spend it in this order:

1. **§10.3, the comparison policy.** Cheapest, no C required, and it protects
   every claim already being made.
2. **§10.2 spec mining** for the kernels that carry the most risk: the
   integrator's truncation control (`cktterr.c`/`CKTterr`) next, since
   `Orders.lean` already has the surrounding machinery; then the *specification*
   of `sparse13`'s pivot policy rather than its code.
3. **§10.4 refactor-and-extract** for exactly one kernel with a bit-exact
   golden — but only once the `f64` gate of [§4](#4-extraction-spike-measured-results)
   opens. Until then this is planning, not work.

## 11. What about c2rust?

Short answer: it does not help. Four independent reasons, and the decisive one is
the same thing that blocks Tier 1 — floats. There is one narrow place where it
would earn its keep, and it is not verification.

### 11.1 It does not address the binding constraint

[§4](#4-extraction-spike-measured-results) measured it: Aeneas's Lean backend has
no floating-point model. c2rust turns C into Rust; it cannot change which types
Aeneas models. Every c2rust-generated line that matters would contain an `f64`
and would fail exactly as `f64_binop` does — `Invalid inputs for binop`.

This is the whole answer, and it is worth stating starkly: even a *perfect*
c2rust translation, with every pointer cleaned up by hand, lands on the wrong
side of an unrelated wall.

### 11.2 It adds a second limitation on top of the first

c2rust's output is raw pointers and `unsafe` by construction, and §4's probe
shows the response to that is explicit:

```
raw_ptr_i32_loop  ->  Aeneas does not yet support dereferencing raw pointers.
```

So c2rust output fails for two reasons instead of one. There is a real *trade*
buried here — c2rust output preserves C's control flow, which is closer to
Aeneas's shape than the port's idiomatic iterators, so it would solve the
nested-loop-return problem — but that problem is not the binding one, and the
memory model it costs is fatal. (And the closure worry that made the trade look
attractive was itself a misreading; see the correction in
[§4](#4-extraction-spike-measured-results).)

### 11.3 The direction of translation is wrong

The project already has a Rust implementation, tested against C. c2rust
manufactures a *third* artifact — a Rust rendering of C whose own correctness is
not proved, only tested — and then either verifies *that* instead of the port, or
tests the port against it. Either way, `port vs C` becomes
`port vs c2rust(C) vs C`, with an unverified translation inserted in the middle.
It strictly increases the number of untrusted links in the chain, which is the
opposite of the goal.

### 11.4 ngspice is a hostile transpilation target anyway

Setting the above aside, the C is not a plausible c2rust input. Measured on the
reference checkout's `src/`:

| Construct | Count |
| --- | --- |
| lines of C | 629,752 |
| `#define` lines | 17,285 |
| `ckt->` dereferences — the global circuit state threaded through everything | 21,263 |
| files using `goto` | 979 |
| function-pointer call sites | 358 |
| files using `va_list` | 34 |
| files using `union` | 18 |
| files using `setjmp` / `longjmp` | 10 / 2 |

c2rust's own known-limitations list names inline assembly, macros, `restrict`
pointers, inline functions and GNU packed structs. But the deeper problem is
structural rather than itemised: ngspice's numerics are driven by a mutable
global `CKTcircuit` reached through a thread-local `ckt` pointer, and SPARSE's
solver is a linked-list arena of C pointers. Transpilation preserves that shape
as raw pointers, which is precisely what Aeneas excludes. The cases where it
would be easy to transpile are the least interesting, and the cases that matter —
the solver — are the hardest and the most pointer-laden.

### 11.5 Where c2rust would actually earn its keep

Not verification — **oracle granularity**.

Today the port's corrector coefficients are pinned two ways: against textbook
fixed-step BDF tables, and against `vandermonde_ag`, which is a *Rust
reimplementation* of C's linear system rather than C. The C-parity tests are
whole-deck rawfile comparisons, which would catch a wrong `CKTag` only
indirectly, through a deck result. There is no direct C-versus-Rust comparison of
the coefficient values, because the coefficient routine is internal to C and
never observable in a rawfile.

c2rust is one way to make C's `nicomcof.c` callable from a Rust test *without
FFI*. That matters because this repository forbids FFI — `unsafe_code =
"forbid"`, and `AGENTS.md` states there is no FFI — so the usual `bindgen` plus
`extern "C"` route is closed. A c2rust-generated and hand-trimmed Rust rendition
of the coefficient routine, compiled into the test binary as ordinary Rust, would
let `tests/` compare the port's `ag` against C's `ag` directly instead of against
a Rust model of C's system.

That is a real improvement in *diagnosis*: it would localise a coefficient
mismatch instead of inferring it from a deck result. It is an engineering task,
not a verification one, and it does not strictly need c2rust — the same effect is
reachable by transcribing C's routine into a test-only module and reviewing it,
which is what `vandermonde_ag` already half-does. c2rust's contribution would be
fidelity-by-construction instead of fidelity-by-review, which is a real but
modest gain for a routine of forty lines.

### 11.6 If a verified solver is the goal, the prover choice is the real question

The float gap is specific to Aeneas's *Lean* backend. Aeneas's float work is
described as adding float support **to the Lean backend**, which implies the
other backends are ahead, and hax also exposes a Coq backend. Nothing here was
tested — Coq is not installed on this machine — so treat this as a flag, not a
finding:

> A Rust→Rocq route through Aeneas/hax may reach floating point earlier than the
> Lean route, at the cost of moving this pilot's specification development to
> Rocq.

Taken together with [§10.1](#101-the-readings-that-do-not-pay-off)'s observation
that the mature *C* tooling is also Rocq-based, the pattern is uncomfortable:
choosing Lean appears to have maximised distance from the available
floating-point models on *both* sides of the port/oracle pair. That does not make
the choice wrong — Lean is where the Rust-side momentum is, and Tiers 0 and 2 are
unaffected — but it deserves to be an explicit decision rather than a drift. It
is the single question most worth settling before any further commitment.

## 12. Gradually moving off C as the oracle

### 12.1 What the oracle actually supplies

"C is the oracle" is three separate jobs, and they have different fates:

| Job | Where it lives today | Can Lean take it over? |
| --- | --- | --- |
| **Defines correct behaviour** | `AGENTS.md`, `MAPPING.md`: the port's specification *is* what ngspice computes | **Yes, component by component.** This is the migration. |
| **Supplies expectations** | 152 files under `conformance/golden`, captured by `cargo xtask golden capture`, consumed by `tests/golden_rawfiles.rs` | **Partly.** Whole-deck rawfiles, no (see below). Component-level values, yes. |
| **Detects fixture drift** | `cargo xtask golden check` re-runs C | Yes, for any component whose expectations become Lean-derived. |

Two facts set the boundaries, and both should be stated up front because they
stop the goal from being "delete the C checkout":

1. **Every existing fixture is a deck plus a rawfile.** `conformance/golden`,
   `conformance/observations` and `conformance/cases` are all whole-simulation
   artefacts. A Lean reference cannot produce them without reimplementing the
   simulator. So **end-to-end parity stays C-defined**, and that is the right
   call: end-to-end behaviour is what users care about, and reimplementing
   ngspice in Lean to check it would be a different project.
2. **The Rust↔Lean link is blocked** ([§4](#4-extraction-spike-measured-results)),
   so Lean cannot yet *check* Rust. What Lean can do today is *define and
   justify* behaviour.

So the migration is not one thing. It is two, and only the first is available:

- **Migration A — move the authority for component behaviour from C to Lean.**
  C stops being what defines correctness and becomes an implementation that is
  cross-checked. Available now for kernels whose behaviour is a pure function of
  small inputs.
- **Migration B — move checking from tests to proofs.** Blocked on the float
  model.

### 12.2 The invariant that keeps this honest

Any migration of authority can quietly lose coverage. Before removing C's
authority for anything, the replacement must satisfy:

> **Every behaviour is defined by something, and nothing is downgraded.** A
> migrated item ends with a Lean artefact that is either *proved* or *validated
> against C*, and a Rust side that is either *tested against that artefact* or
> *proved against it*. C's authority is withdrawn only after the Lean artefact
> has been shown to agree with C on a grid.

That last clause is the load-bearing one. It is also where the wrinkle from
[§11.5](#115-where-c2rust-would-actually-earn-its-keep) bites: for a routine
*internal* to C — `CKTag` is the example — you cannot call C directly, so
co-validation is either indirect (through deck-level parity, which already
exists) or requires the callable-C work of §11.5. Either is acceptable; leaving
it unstated is not.

### 12.3 The mechanism: a component-expectation fixture

There is no component-level fixture category today: every directory under
`conformance/` is deck- or parser-level. Creating one is the first concrete
artefact of the migration.

| Piece | Role |
| --- | --- |
| `conformance/components/<name>.*` | Expected values for one component-level pure function over a fixed input grid. Committed, like a golden. |
| A Lean **emitter** under `formal/emitters/`, run with `lake env lean --run` | Writes that fixture. The emitter is the proved (or proved-then-validated) reference. |
| A Rust test under `tests/` | Calls the port's component API and compares against the fixture within a tolerance. |
| An opt-in `tests/c_<name>_reference.rs` | Validates the same grid against live C, so the emitter is shown to agree with the oracle it is displacing. |
| `cargo xtask components check` | Mirrors `golden check`: regeneration is deliberate and reviewable, per the existing rule that goldens are never silently recaptured. |

Why a fixture and not just another theorem: a theorem proves a *law*, and a
law cannot be checked against Rust without extraction. A fixture pins
*agreement on a grid*, which a test can check today. The pair is what lets C's
authority be withdrawn for that behaviour — the theorem says the law holds, the
fixture says this implementation produces these numbers, and C's role drops to
confirming the emitter.

### 12.4 Phases, with exit criteria

**Phase 0 — specification proofs. Done.** 22 declarations, no dependency on
extraction, plus the `xmu` oracle experiment ([§10.5](#105-worked-example-the-xmu-rule)).
Nothing here is at risk from the float block.

**Phase 1 — build the fixture mechanism on one kernel.** The corrector
coefficients are the right pilot, for a specific reason: `IsCorrector`,
`Orders.lean` and `Theta.lean` already supply the law, the Rust surface is a
single function (`StepHistory::trial`, `Coefficients::ag`), and — the point —
the coefficients are currently pinned against *hardcoded textbook BDF tables and
`vandermonde_ag`, a Rust reimplementation of C's linear system*, not against
anything proved and not against C. This is the weakest-evidenced kernel with a
proved specification attached, which makes it the best first migration.

Exit criteria:

- a Lean emitter computes `ag` for a grid of step sequences (orders 1–6,
  uniform and non-uniform steps, including the `xmu` trapezoidal path);
- `conformance/components/corrector_coefficients.*` is committed;
- a Rust test compares `Coefficients::ag` against it, and becomes the primary
  evidence in place of `vandermonde_ag`;
- C co-validation is either recorded as indirect (deck parity) or made direct
  via §11.5 — **a decision, not a default**;
- `cargo xtask components check` exists.

*Cost: a Lean emitter plus a fixture plus a test plus an xtask subcommand. Small
enough to be a single PR, which is what makes it a good pilot.*

**Phase 2 — start the coverage table before migrating further.** The metric that
makes "gradual" objective is **authority coverage**: for each component
behaviour, whether the definition of correct is C, a Lean spec, or a Lean
reference validated against C. Write the baseline table *first*
(`docs/port/ORACLE_COVERAGE.md`, or a section of `VERIFICATION.md`), then
migrate in risk order. Suggested order after the coefficients:

1. truncation control (`cktterr.c` / `CKTterr`) — already has a tested formula
   and `Orders.lean` has the surrounding machinery;
2. device stamp **structure** — which rows and columns are touched, signs,
   branch ranges, and which device state is written. Float-free in essence even
   though the entries are `f64`, and a large share of real simulator bugs;
3. MNA allocation and branch binding (Tier 2) — ground elimination, node-before-
   branch row order, contiguous per-device branch ranges.

Per-item exit criteria: proved specification, fixture, Rust test, authority row
updated, C's role reduced to co-validation.

**Phase 3 — retire `golden check` where authority has moved.** Once a
component's expectations are Lean-derived and its fixture is stable, its C
regeneration stops; C remains only in opt-in co-validation tests. The measure is
the count of behaviours whose authority is Lean, and the count of C-derived
fixtures still being regenerated should trend to zero for migrated components.

**Phase 4 — move checking to proofs. Blocked**, on Aeneas's float model
([§4](#4-extraction-spike-measured-results)). When it lands, add refinement per
kernel. Then, for that kernel: the fixture stops being the primary evidence and
becomes a regression net, and the exit criterion is that the Rust body is
*proved* to compute the Lean specification.

### 12.5 What "verified" will actually mean, and it is not what people expect

Worth deciding now, because it changes how Phase 4 is planned. When floats lower
into Lean they lower to an **opaque** scalar: Lean's `Float` carries no IEEE
lemmas, mathlib's float formalisation is experimental, and the upstream draft
itself notes that "lots of core routines are `opaque`". So a refinement proof
will be able to establish:

- index, row and column correctness, branch ranges, loop invariants and
  termination, which device state is written, ordering, aliasing and
  traversal order;

and will **not** establish arithmetic identities. That is still worth a great
deal — most simulator bugs are bookkeeping rather than floating-point identities
— and it composes cleanly with Tier 0: the *scheme* is proved correct over `ℝ`,
the *implementation structure* is proved to match the scheme, and the arithmetic
is a stated assumption ("`f64` operations behave as IEEE 754 doubles") rather
than a proof. Closing that last gap needs Flocq-style error bounds (a Lean port
exists: `FloatSpec`); it is research-grade and should not be on the critical path.

The honest form of any claim is therefore per kernel and explicit about which of
the three links is proved and which is assumed. "Verified simulator" will not be
available, and asking for it is how this kind of effort fails.

### 12.6 What to measure, and when to stop

- **Primary metric: authority coverage** — component behaviours whose definition
  of correct is Lean rather than C. This is what "gradual" means, and it needs a
  baseline table before Phase 2, not after.
- **Secondary: C-derived fixtures regenerated per release**, which should fall to
  zero for migrated components.
- **Stop conditions:**
  1. If Aeneas never lands a usable float model, the outcome is
     *component-level specification authority only*. Still worth having, but it
     must be labelled that way rather than drifting into a claim about the
     simulator.
  2. If reference-transcription cost exceeds the risk reduction, Phase 2 slows
     rather than continues. The pilot's data point: the integrator's *algebra*
     took about a session per kernel; device stamps will be substantially more,
     and each needs its own fixture and co-validation.
  3. **Whole-deck parity is never migrated.** If a Lean simulator becomes the
     goal, that is a new project with a new justification, not Phase 5 of this
     one.

### 12.7 Order of work

1. **Settle the prover question** ([§11.6](#116-if-a-verified-solver-is-the-goal-the-prover-choice-is-the-real-question)).
   It gates Phase 4, it is cheap to test (install Coq, re-run the extraction
   probe against the Coq backend), and getting it wrong invalidates the later
   phases.
2. **Phase 1**, the component fixture on the corrector coefficients. Small, and
   it makes the inversion concrete rather than aspirational.
3. **Phase 2's baseline table**, before migrating anything else.
4. **Continue Tier 0 and Tier 2 proofs**, which pay off regardless of everything
   above.
5. **Re-run `formal/probes/extraction_shape_probe.sh` after every hax release**,
   and start Phase 4 the release after its `f64_*` rows turn green.

### 12.8 The bottom line

C probably cannot be retired, and does not need to be. The realistic target is
this:

> **Component behaviour is defined by proved Lean references; end-to-end
> behaviour is defined by C parity; C is co-validated rather than authoritative;
> and the Rust implementation is proved to match the Lean references for the
> kernels where that becomes possible.**

That is a real, incremental programme with a measurable metric, and every step of
it is useful on its own. Chasing "delete the oracle" would require a Lean
simulator, which is not this project.
