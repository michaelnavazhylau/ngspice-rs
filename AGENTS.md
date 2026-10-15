# Agent guide for ngspice-rs

## Start here

This is a from-scratch Rust port of ngspice, not a Rust wrapper around its C
library. Implement behavior incrementally and keep unsupported cases explicit.

Before editing:

1. Check the working directory, Git branch, status and existing changes. Never
   discard unrelated work or assume sibling checkouts have identical contents.
2. Read the relevant code and tests, then consult:
   - `docs/port/ARCHITECTURE.md`: module boundaries and design rules.
   - `docs/port/MAPPING.md`: upstream C references and licensing boundaries.
   - `docs/port/ROADMAP.md` and `docs/port/TODO.md`: milestones and pending work.
   - `docs/port/VERIFICATION.md`: conformance and external-oracle workflows.
   - `docs/port/NATIVE_SOLVER.md`: implemented solver APIs, how analyses
     drive them, numerical/error policies and limits (on branches before M10
     wave 0.5: `DIFFSOL_FAER_IMPLEMENTATION.md`).
3. Verify capabilities against the current branch. Some introductory documents
   retain historical scaffold status; parsing a device does not imply that its
   simulation backend exists.

## Checkout and publication layout

A standalone clone needs only this repository. In the local development setup,
its parent directory may also contain:

| Directory | Purpose |
| --- | --- |
| `worktrees/ngspice-rs/` | Rust development worktree with upstream C reference sources |
| `worktrees/ngspice-rs-diffsol-faer/` | Separate solver-integration worktree |
| `worktrees/ngspice-rs-*/` | Rust-only milestone and experiment worktrees |
| `ngspice-rs-dist/` | Primary Rust-only GitHub publication checkout |
| `ngspice_test/` | Primary upstream C checkout/reference build |

These paths are optional, not build dependencies. C-tree and Rust-only Git
histories differ: do not push the C worktree/history into the standalone GitHub
repository. Inspect remotes and the intended PR base before publishing. Transfer
only relevant Rust-port changes, preserving newer target-side changes. The
C-tree checkout may provide `scripts/publish-rust-only.sh`; inspect its current
branch mapping and clean-target requirements before using it. From a linked
worktree under `worktrees/`, explicitly pass `--to ../../ngspice-rs-dist`
(relative to the worktree root), or set `NGSPICE_RS_DIST` to an absolute path;
the script's legacy sibling-directory default does not match this layout.
Keep linked checkouts under `worktrees/` and use Git's worktree commands to
create or relocate them; leave the primary repositories at the parent root.

Leave upstream C sources untouched unless the task explicitly targets C.
Read them as behavioral references and run the C executable out of process.
Do not commit build output, temporary oracle files, `target/`, or credentials.

## Architecture and ownership

Keep dependencies acyclic and responsibilities in their owning module. The port
was six crates and is now one package, `ngspice-rs`, so the old crate boundaries
are the module tree under `src/` and **nothing enforces the direction any more**:
reviews must.

| Module | Owns |
| --- | --- |
| `src/primitives` | Dependency-free numbers, complex values, nodes, errors, analysis taxonomy |
| `src/netlist` | Deck loading, positioned tokens, winnow grammars, semantic AST |
| `src/maths` | Dense/sparse/complex storage, backend adapters, LU, numerical integration |
| `src/devices` | Device factories, circuit topology, branch binding, physical equation stamps |
| `src/analysis` | Analysis orchestration, convergence policy, accepted points, plots and rawfiles |
| `src/cli` | CLI commands, coverage reporting and exit-status contract; the `spice-rs` binary is `src/bin/spice-rs.rs` |
| `xtask/` | Development automation and external C golden capture/checks (separate, unpublished workspace member) |

`src/netlist` may not use `maths`/`devices`/`analysis`; `src/maths` may not use
`netlist`/`devices`; nothing may use `cli`. Do not put device semantics into
numeric backend adapters or make lower-level modules depend on analysis/CLI code.

## Implementation conventions

- Rust edition 2024; respect the MSRV in `Cargo.toml` (currently 1.89).
  Avoid APIs or syntax newer than that minimum.
- `unsafe_code` is forbidden. Do not add FFI, external native solver build
  requirements or solver crates casually: the linear algebra is a Rust port of
  ngspice's SPARSE 1.3 and the only dependencies are winnow and petgraph.
- Use workspace dependencies and preserve `Cargo.lock`. Explain dependency or
  feature changes, including their minimum-Rust and licensing consequences.
- Preserve `COPYING`, `AUTHORS` and `NOTICE`. Do not copy LGPL KLU algorithms
  into this BSD-licensed implementation; SPARSE 1.3 (Berkeley notice, kept in
  `src/maths/sparse13.rs` and `NOTICE`) is the solver to follow.
- Document public APIs. Nontrivial SPICE behavior should name its upstream C
  file/function; explain deliberate divergence instead of guessing parity.
- Return `SpiceResult` errors for invalid or unsupported input. Pending ports
  use `SpiceError::NotYetPorted` with a useful C reference; unsupported
  combinations use explicit errors, numerical failures retain backend context.
- Never report partial success by silently ignoring cards, parameters, devices,
  options or solver failures. Avoid `todo!()`, `unimplemented!()` and input-driven
  panics; validate dimensions before calling backend APIs that assert them.
- Keep changes focused. Update coverage/status documentation and stub-expectation
  tests only when corresponding production functionality actually works.

## Parser contracts

- Use winnow combinators in the semantic parser rather than a second manual
  cursor/parser implementation. Preserve the physical-line loader and tokenizer
  contracts unless the task explicitly changes them.
- Preserve source locations and byte-based columns, numeric spelling, terminal
  order, ground-alias settings and case-folding behavior.
- AST parameter assignments are ordered textual values. Preserve duplicates
  and C setter precedence; do not replace them with an unordered map.
- Commit recognized syntax with appropriate cuts. Optional/repeated parsing
  must not swallow missing values, overflow, expressions or unsupported forms.
- Require complete card consumption except documented C-compatible termination
  rules. Keep declaration lookup local to each deck; do not confuse model-name
  disambiguation with model validation or backend availability.
- Preserve CLI exits: 0 success, 1 usage error, 2 failure, 3 not yet ported.

## MNA, topology and numeric guardrails

- Ground is eliminated from MNA. Matrix row 0 is an ordinary unknown, not
  ground. Circuit node IDs, device ordinals, graph handles and matrix rows are
  different namespaces.
- Allocate node rows before branch-current rows and preserve per-device branch
  ranges. Source/inductor current is positive from the first terminal to the
  second; document and test signs and plot names.
- Prefer petgraph storage and algorithms over custom traversal implementations.
  Circuit incidence and matrix-row coupling have different meanings. Generic
  connectedness is neither a conductive DC-path test nor proof of nonsingularity;
  disconnected blocks may be individually nonsingular.
- Use general LU, not Cholesky, for MNA: the SPARSE 1.3 port
  (`src/maths/sparse13.rs`). Its branch constraints are indefinite and stamps
  can be nonsymmetric. Keep SPARSE's control flow (pivot candidates, tie
  breaking, element creation order) so pivot sequences match C.
- Keep owned factor snapshots separate from cloneable stamping storage. If a
  cache is introduced, define and test invalidation for every mutation.
- Keep one pivot order per circuit pattern (C's one `CKTmatrix`): refactor with
  it (`spFactor`), reorder when the element set changes, and after a zero
  pivot on refactor reorder as `NIiter`/`NIacIter` do. AC shares the real
  frame. `.options pivtol`/`pivrel` reach every reorder; hard-code thresholds
  only where C does (pole-zero's `1e-30`).
- The error policy is C's, from SPARSE's status (`spsmp.c`, `niiter.c`):
  `spSINGULAR`/`spZERO_DIAG` fail, naming the unknown as `NIiter` does
  (`singular matrix:  check node X`); `spSMALL_PIVOT` is accepted (C's `OK`)
  with a solver warning; a circuit unknown with no matrix element keeps its
  RHS as C's untranslated row does (standalone matrices reject it). Do not add
  rank probes, condition estimates or residual checks: C has none, and they
  reject or alter circuits C simulates (`SPARSE_RANK_DIAGNOSTICS.md` is
  superseded). Still validate dimensions, finite inputs, duplicate-sum
  overflow, RHS and non-finite solutions.
- Degenerate circuits must fail as C does (same error class and unknown) or
  give C's answer, never a silent different one; `tests/solver_robustness.rs`
  holds the cases. Where C recovers through a fallback the port lacks (e.g.
  `OPtran`), fail explicitly and say so.
- AC needs complex `A + j*omega*E` assembly and complex LU with explicit scalar
  conversions, not a real solve or time integration.

## Transient boundaries

- The only `.tran` backend is the trapezoidal/Gear companion driver
  (`src/analysis/companion.rs`), a port of `dctran.c`: `NIiter` at every
  timepoint, `nicomcof.c`/`niinteg.c` coefficients, `CKTterr` truncation
  control and C's breakpoint table. Follow C's control flow and operation
  order rather than inventing step-control policy.
- `backend=diffsol` (the removed adaptive BDF backend) and `method=bdf` are
  explicit errors raised at dispatch, whatever the deck options. Do not add a
  second integrator without a design decision; the DAE/BDF work is
  recoverable from Git history (tag `v0.4.0`).
- Keep the immutable `E*x' + A*x = b(t)` assembly (linear DC/AC) separate from
  timestep-dependent companion stamps; never discretize a device twice.
- Stop at source and device breakpoints (`CKTsetBreak`), restart history as C
  does and distinguish left/right limits. Never interpolate across
  discontinuities or commit device state during rejected trials. Call
  `Device::accept` only for accepted points, never for rejected trials.
- The plot holds every accepted point, as C's rawfile does; do not resample it
  onto the requested grid. Enforce progress, the step budget (`maxsteps`),
  final-time and maximum-step constraints; use distinct voltage/current
  absolute tolerances and propagate failures.

## Validation commands

Run from the selected Rust workspace root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

For compiler/dependency changes, also validate minimum Rust:

```sh
cargo +1.89.0 test --workspace --locked
cargo +1.89.0 clippy --workspace --all-targets --locked -- -D warnings
```

`cargo xtask ci` is the existing convenience command; explicit commands above
also cover the lockfile and all-target lint requirements. For offline work,
cache dependencies with `cargo fetch --locked` first. Ordinary tests use committed
fixtures and do not require a C toolchain.

Opt-in C checks require an absolute path to a built reference binary:

```sh
NGSPICE_BIN=/absolute/path/to/ngspice cargo xtask golden check
NGSPICE_BIN=/absolute/path/to/ngspice cargo test -p ngspice-rs --test c_reference --locked -- --ignored
NGSPICE_BIN=/absolute/path/to/ngspice cargo test -p ngspice-rs --test c_linear_reference --locked -- --ignored
```

Locally the reference may be `../ngspice_test/build/src/ngspice` from this
primary checkout, or `../../ngspice_test/build/src/ngspice` from a linked
checkout under `worktrees/`; verify it exists and resolve it to an absolute path.
`golden capture` overwrites committed fixtures: use only deliberately and review
all changes. Do not regenerate goldens or loosen tolerances to conceal failures.
`golden check` reproduces C output; it does not by itself validate Rust simulation.

## Regression expectations and handoff

- Test production interfaces, not only backend smoke tests: pivoting, repeated
  RHSs, mutation after factorization, pattern changes, dimension/empty/nonfinite/
  overflow errors, singular homogeneous systems and residuals.
- Cover ground elimination, branch signs, supported independent blocks and
  invalid ideal-source loops. Preserve topology and rawfile round-trip tests.
- Compare DC/AC values by variable name with justified relative **and** near-zero
  absolute tolerances; do not assume C and Rust use identical vector order.
- Compare transient results on a common physical sample grid, not identical
  internal timestep sequences. Include analytic RC/RL/RLC, initialization
  (`MODETRANOP`, `.ic`/`uic`), source events, accepted-point semantics and
  error/limit paths as relevant.
- Preserve rawfile layout and document known decimal round-trip limitations.
- Before handoff, inspect `git diff --check` and the final changed-file list.
  State what changed, tests actually run, unsupported cases and remaining risks.
  Do not claim full SPICE parity or milestone completion from a bounded subset.
- Commit, push, open/merge PRs or recapture external data only when requested.
