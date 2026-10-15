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

---

## 1. Recommendation

**Worth doing, at specification level, for the numerical kernels — and not
worth attempting end-to-end.**

Three separable activities are usually conflated under "formal verification".
They have wildly different costs, and only the first is cheap:

| Activity | Cost | What it buys |
| --- | --- | --- |
| **Specification proofs** over `ℝ`: state what each kernel must compute and prove the scheme has that property, for all inputs and all step sequences. | Low. The pilot did the integrator in an afternoon. | Turns sampled numerical tests into theorems; explains *why* the C-parity results hold; pins down anti-properties (e.g. "order 2 is *exactly* order 2") that tests can only sample. Does not touch the Rust source. |
| **Refinement proofs** over `f64`: prove the Rust function body computes the spec, including rounding. | Very high, and for the linear algebra probably infeasible with today's tooling. | What most people mean by "verified simulator". |
| **Structural proofs** over the combinatorics: MNA ground elimination, branch ordering, pattern/pivot-order determinism. Float-free, so both cheap *and* genuinely about the code. | Medium, gated on being able to model the device layer. | Catches a real class of bugs (index spaces, sign conventions, ordering) that testing catches only by accident. |

The recommendation is to fund the first and third, treat the second as
research, and be explicit in any public claim that specification proofs are
specification proofs. The existing C-differential test suite is the right
instrument for the second tier and already does most of the available work;
proof should be aimed at what testing structurally cannot reach.

## 2. What the pilot established

Concrete, reproducible facts from building `formal/`:

| Observation | Measurement |
| --- | --- |
| Lean toolchain | Already installed on this machine: Lean 4.35.0 + Lake 5.0.0 via `elan`. No new toolchain needed. |
| mathlib dependency | `master` @ `6015dbb`, 9 039 cached oleans, **7.2 GB** of build artifacts. One-off `lake exe cache get` took several minutes on a fast connection. |
| Incremental build of the four project modules | **~5 s**, and a single file is ~1.5 s. Editing proofs is interactive-speed. |
| Proofs written | 12 public declarations across 3 modules; zero `sorry`, zero `axiom`. `#print axioms` reports only `propext`, `Classical.choice`, `Quot.sound`. |
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
| Same, plus `--include 'ngspice_rs::maths::polynomial::evaluate'` | Charon now produces a 408 KB LLBC containing the real MIR, including the closure `evaluate::closure` and its `call_mut` body. Aeneas then fails: `Invalid inputs for binop` at **`src/maths/polynomial.rs:66:48-66:55`** — i.e. the `sum * x` inside `\|sum, c\| sum * x + c` — and warns that `DoubleEndedIterator`/`Iterator` are mutually recursive trait declarations, which it does not support. |
| `--start-from`/`--include` on `ngspice_rs::maths::integrator::gear_corrector` | Aeneas fails with `Returns inside of nested loops are not supported yet` at `src/maths/integrator.rs:393:0-397:1`, then ignores the body. |

### What it means

The two functions I picked as the *most* likely Tier-1 successes both fail, for
two unrelated reasons, and both failures are in Aeneas's Rust-feature support
rather than in anything specific to ngspice-rs:

- **`evaluate` fails on the closure.** The failure is not in the fold or in
  `f64`; it is that a captured `f64` inside a closure body does not typecheck in
  Aeneas's interpreter. `iter().rev().fold(…)` is one of the most idiomatic
  shapes in Rust and is used ~1 374 times in this tree.
- **`gear_corrector` fails on control flow.** It has no closures and no traits:
  fixed-size arrays, `for` loops, `as`-free arithmetic — on paper the ideal
  Aeneas target. Aeneas still rejects it, because mutually nested loops with
  interior control flow are outside its `RustM`/`ControlFlow` model at this
  version.

So the refinement tier is **not** currently gated on effort or on learning the
tools. It is gated on Aeneas's supported Rust subset, and the subset this tree
needs is on the wrong side of that line. Three consequences, in order of
importance:

1. **Tier 1 should not be planned as "learn hax, then extract modules".** The
   honest estimate for the current toolchain is that most kernels in
   `src/maths/` are not extractable at this version, and the effort would be
   spent upstream-tracking rather than proving.
2. **The mitigation that *does* exist is narrow and must be scoped carefully.**
   A closure-free, single-loop formulation of a kernel would likely extract:
   `evaluate`'s body can be rewritten as `let mut sum = 0.; for c in
   coefficients.iter().rev() { sum = sum * x + c }`, which removes both the
   closure and the nested control flow. For a handful of small helper kernels
   that is acceptable. For `maths/sparse13.rs` it is **not**: the file
   deliberately mirrors C's loop structure and operation order so that pivot
   sequences match SPARSE, and AGENTS.md makes that a hard requirement.
   Refactoring for extraction would trade a proved property for a lost one.
3. **Re-probe cheaply before committing.** The next sensible experiment is a
   deliberately closure-free, single-loop, early-return-free function — either
   a rewrite of `evaluate` or a small new helper — to establish whether *any*
   kernel of real interest is extractable today. That is a ~30 s run once the
   candidate is chosen. Until it succeeds, Tiers 1 and 3 should be treated as
   blocked and the project should invest in Tiers 0 and 2 instead.

One incidental finding worth recording: **`--start-from` without `--include`
silently yields `<opaque>`.** The extraction "succeeds" (exit 0, files
written) while containing nothing. Any scripted extraction must assert that the
expected function appears in `Funs.lean`, or it will report success on an
empty artifact.

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
   12 declarations, no `sorry`, `#print axioms` clean, warning-free, with a CI
   job that caches mathlib. Add the `.gitignore` entry (done). Treat it as
   documentation of the numerical kernels, not as a publication claim.
   *Cost: review only.*
2. **Finish the extraction probe to a decision.** [§4](#4-extraction-spike-measured-results)
   already establishes that the two most plausible candidates fail. Complete the
   picture with one deliberately closure-free, single-loop, early-return-free
   function and find out whether the pipeline works on *anything* of interest in
   this tree. Deliverable: a yes/no on Tier 1, plus a note on which Aeneas
   version to re-probe against. *Cost: ~30 s per attempt once the candidate is
   written, so hours, not days.*
3. **If M2 is a yes, extract exactly one kernel** — the smallest one with a
   Tier 0 theorem already attached, so the proof obligation is "this body
   equals the model I already specified" rather than new mathematics. Given
   `horner_eq_sum`, a closure-free `evaluate` is the obvious choice, and it
   validates the whole approach end to end. *Cost: days, not weeks, if M2 is
   positive.*
4. **Prove the MNA allocation invariants** (Tier 2) as specifications over an
   abstract device interface, and check them against the Rust construction
   logic by inspection plus targeted tests. This avoids `dyn Device` by
   modelling the *interface*, not the concrete devices — the cheapest way to get
   value from the structural tier, and it does not depend on M2. *Cost: weeks.*
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

