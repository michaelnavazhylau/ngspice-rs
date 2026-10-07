# Rust port TODO

This is the **central implementation checklist**. Keep task status here;
[README.md](README.md) summarizes capabilities, [ROADMAP.md](docs/port/ROADMAP.md)
defines milestone exit criteria, and [VERIFICATION.md](docs/port/VERIFICATION.md)
describes the C-oracle workflow. Parsing a card does not imply simulation support.

## Current checkout and mainline status

GitHub `main` contains the solver implementation at `31e245f`, merged by
`b467ca0`. The local C-reference development `main` at `d3c8cccf4` predates it;
that history and the Rust-only history are distinct. The solver development
revision is `05f9eb8a9` on `inspect/diffsol-faer`. These are reference revisions,
not build dependencies. Do not replace newer public code with an older export.

PR #51 (`cdc078c`) merged #11 passive syntax and #17 initial model infrastructure;
their acceptance suite passes on this main-based branch. This checkout implements
bounded passive elaboration (#19), pending merge. Completed entries describe the
checkout, not a claim that all M2 slices have already merged.

| Area | Current checkout |
| --- | --- |
| Parser | Scalar R/C/L/V/I, declared-model passives, scalar models, bounded D/Q/M; six of eight fixture decks |
| Model inputs | Top-level first-wins resolver, family/level checks, bounded passive factories and diode input schemas |
| Passive models | R sheet/C area-perimeter geometry, scalar model L, TC1/TC2/TEMP/TNOM, scale and multiplicity |
| Topology | Petgraph circuit incidence and matrix-row graphs, simulation branch-row binding |
| Linear equations | faer real/complex LU, scalar R/C/L/V/I elaboration and stamps |
| DC / AC | Linear `.op`, one-source `.dc`, complex RLC `.ac` |
| Transient | Explicit diffsol adaptive BDF; restricted diagonal mass structure and nonsingular algebraic block |
| Nonlinear devices | D/Q/M syntax and initial diode input validation; no equations |
| CLI | Inspection/parsing; simulation through APIs/examples only |

**No full M1/M3 completion or full SPICE parity is claimed.** The implemented BDF
is not ngspice trapezoidal or fixed Gear-2. Its Step/Pwl source waveforms are
available through the device API, not netlist syntax; floating/coupled capacitor
DAEs, higher-index constraints, nonlinear charge, `.ic` and `uic` remain unsupported.

[DIFFSOL_FAER_IMPLEMENTATION.md](docs/port/DIFFSOL_FAER_IMPLEMENTATION.md) records
245 passing tests and five separately passing opt-in C checks for the integrated
solver work. This historical report is separate from later prerequisite validation
in [MODEL_SCHEMAS.md](docs/port/MODEL_SCHEMAS.md).

## Completed front end and infrastructure

- [x] M0: workspace/crate contracts, errors, CLI exit contract, CI task and out-of-process C golden harness; initially dependency-free.
- [x] M1a: scalar R/C/L and DC/AC V/I ASTs, terminal canonicalization, ordered textual parameters, source positions and `.end` termination.
- [x] M1a: parser regressions, CLI process-exit tests and opt-in live C scalar oracle.
- [x] Winnow semantic-parser rewrite with committed-error, lookahead and full-consumption regressions; loader/tokenizer contracts retained.
- [x] Scalar `.model` name/type/raw first level/ordered assignments for D/BJT/MOS/R/C/L (`inpdomod.c`, `inpgmod.c`); AST stays raw; schema/selector validation is device-owned.
- [x] Two-terminal D syntax and scalar geometry (`inp2d.c`), leading-area precedence and `perim` alias; diode fixture and C scalar oracle.
- [x] Three/four-terminal Q and four-terminal M syntax (`inp2q.c`, `inp2m.c`), declared-model disambiguation, forward references and ordered scalars.
- [x] BJT/MOS fixture/CLI regressions and live scalar/terminal-binding oracle, including numeric BJT model rejection checks.
- [x] Production petgraph circuit-incidence and assembled matrix-row coupling graphs, including isolated vertices, parallel ports and cancelled stamps.
- [x] Ground/node/device/row namespace and snapshot-mutation regressions; connectivity is not a solvability test.
- [x] Correct guide references for expression grammar, deck dispatch, numparam and circuit input passes.
- [x] Branch-aware Rust-only publication; feature branches cannot overwrite public main.

## Completed linear engine on GitHub main

These are implemented, **not new implementation tasks**. Retain their production
tests and documented limits as the remaining functionality is added.

- faer dense pivoted LU, sparse real/complex LU, owned factor snapshots and repeated-RHS solves; finite/dimension/rank/residual diagnostics and exact-pattern symbolic reuse.
- Literal scalar R/C/L/V/I factories, AST-to-circuit elaboration, ground elimination, node-before-branch numbering, branch signs and immutable `E*x' + A*x = b(t)` assembly.
- Linear `.op`, one-independent-source `.dc` with reused factors, and complex `.ac`; unsupported combinations fail explicitly.
- Explicit `.tran ... backend=diffsol method=bdf`, separate from trap/Gear companion integration, with bounded DAE initialization.
- Device-API Constant/Step/Pwl forcing, breakpoint stops/history restarts, right-hand event states, accepted-point callbacks, requested output grids and step/work limits.
- Production linear C-golden comparisons, analytic RC/RL/RLC transients and a common-grid live C Pwl RC comparison; stable and Rust 1.89 validation recorded in the implementation guide.

## 1. Linear-core validation and maintenance — M2

- [x] Integrate the linear solver work into GitHub main, including its implementation guide, dependency/license rationale, Rust 1.89 requirement, CI matrix and production-interface tests.
- [x] Add production `.op` comparisons against the RC-divider/RLC C goldens at justified relative and near-zero absolute tolerances.
- [ ] Keep formatting, all-target Clippy and workspace tests green on stable and Rust 1.89; rerun opt-in C checks deliberately when functionality changes.
- [x] Reconcile historical architecture/mapping/recommendation and public API docs with the bounded linear engine and Rust 1.89 MSRV (GitHub #49); no full M1/M3 completion claim.
- [x] Implement bounded model-backed passive elaboration (GitHub #19) on merged #11/#17: typed support table, R sheet/C area-perimeter geometry, scalar model L, contextual temperature, scale/multiplicity, atomic failures and production DC/AC/C checks; pending merge. See [PASSIVE_MODELS.md](docs/port/PASSIVE_MODELS.md).

## 2. Complete the netlist front end — M1

### M1b: remaining model/device and waveform syntax

- [ ] Add required remaining model/diode and instance flags; keep thermal/CIDER, extra ports, binning and advanced-backend gaps explicit.
- [ ] Support required vector IC forms while preserving ordered setter precedence.
- [x] Parse declared-model R/C/L without mistaking models for parameter references (#11); forward references, omitted values and bounded geometry-only forms, not arithmetic.
- [ ] Represent source waveforms, starting with `PULSE`; keep syntax separate from time evaluation.
- [ ] Pin malformed/unsupported variants and enable the transient fixture AST test.

### M1c: subcircuit and file structure

- [ ] Parse `.subckt`/`.ends` ports, formal parameters, scope and X instances.
- [ ] Diagnose unmatched/mismatched terminators and duplicate definitions; decide nested-scope representation before accepting nested subcircuits.
- [ ] Parse `.include`/`.lib` paths and sections, preserving quoted paths.
- [ ] Implement source-relative resolution, recursion/cycle limits and source-location provenance.
- [ ] Use directed petgraph dependency graphs/SCC/toposort for file/subcircuit and parameter dependencies, not a custom graph engine.
- [ ] Enable `subckt_divider` AST tests; parsing is not flattening.

### M1d: parameter semantics and full front-end gate

- [ ] Implement a bounded `.param` expression grammar/evaluator from numparam behaviour.
- [ ] Define evaluation order, scope, units and undefined/cyclic-reference diagnostics.
- [ ] Parse `.option` and `.global`; implement required ground scope rules.
- [ ] Add normalized-deck serialization preserving source parameter application order.
- [ ] Commit deterministic token/AST snapshots and document regeneration.
- [ ] Round-trip all eight rawfile fixture decks through AST and normalized text.
- [ ] Keep `cargo xtask ci` green and mark M1 complete only after every exit gate passes.

## 3. Model elaboration and validation

- [x] Top-level first-declaration model lookup, family compatibility and family-specific level selection/rounding (#17); failed elaboration leaves circuit state unchanged.
- [x] Device-owned scalar schema extension API and bounded diode IS/N/RS/AREA/TEMP/TNOM defaults/ranges (#17); raw AST preserved; D/Q/M factories still unavailable.
- [ ] Add scoped model resolution, binning and further device-owned schemas/defaults; expand only with production tests.
- [ ] Preserve omitted BJT substrate semantics and explicitly diagnose unavailable device backends.
- [x] Define and implement the bounded passive value/geometry/temperature surface (#19); explicit errors for unsupported setters, missing/invalid geometry and nonfinite derivations.
- [ ] Expand passive aliases, coil geometry, DTEMP/TCE/AC-only values and other advanced forms only with documented formulas and conformance tests.

## 4. Complete SPICE-compatible transient analysis — M3

Complex linear AC is already implemented on main. Bounded adaptive
BDF does not close the following trap/Gear and general-transient requirements.

- [ ] Implement trapezoidal and Gear-2 integration (`src/maths/ni/`) and timestep/truncation-error control.
- [ ] Implement C/L companion models without double-discretizing diffsol equation stamps.
- [ ] Implement `.ic`, `.nodeset`, instance IC and `uic` semantics with consistent constraints/derivatives.
- [ ] Connect parsed source waveforms to time evaluation and breakpoint handling, including `PULSE`.
- [ ] Demonstrate formulations/tests for floating/coupled capacitor networks and higher-index source constraints before enabling general DAEs.
- [ ] Retain accepted-state/trial-state separation, event left/right limits, voltage/current tolerances, output-grid separation and progress/work budgets.
- [ ] Complete RC/RLC transient and AC C-golden exit gates on common physical sample grids; do not require identical adaptive timesteps.

## 5. Nonlinear devices and convergence — M4

- [ ] Implement diode, BJT and MOS level-1 physical equations and Jacobian stamps.
- [ ] Add Newton iteration, device limiting, source stepping, gmin stepping and convergence options.
- [ ] Extend DC sweeps beyond the delivered single-independent-source linear subset as required, with explicit supported sweep targets/combinations.
- [ ] Implement nonlinear bias-linearized AC and transient charge/flux with designed trial-state ownership and accepted-state commits.
- [ ] Validate diode rectifier/DC, MOS inverter and BJT bias results against C at justified tolerances.
- [ ] Keep advanced MOS/BSIM-family scope explicit before nonlinear expansion.

## 6. Usability and output — M5

- [ ] Add a CLI simulation command that elaborates supported decks, runs analyses and writes results while preserving exits 0/1/2/3.
- [ ] Implement subcircuit instantiation/flattening, parameter passing, model/node scoping and `.global` semantics.
- [ ] Add `.measure` (or a deliberate substitute), `.print`/`.save` output selection and `.four`.
- [ ] Add binary rawfile support so unmodified C goldens can be consumed.

## 7. Verification, numerical follow-up and documentation

- [x] Add `cargo xtask golden verify` for the three supported `.op`/`.ac` fixtures, comparing metadata and named real/complex components with the existing DC/AC bounds; explicit exclusions, failure diagnostics and process tests (GitHub #7).
- [ ] Expand production C comparisons as parser/device/analysis support lands; keep ordinary tests independent of a C toolchain.
- [ ] Preserve singular homogeneous/source-loop, disconnected valid block, mutation, pattern-change, finite/overflow and residual regression coverage.
- [ ] Investigate equilibration for ill-scaled MNA and reduce the n-extra-solves sparse rank-diagnostic cost without weakening uniqueness checks.
- [ ] Keep this checklist authoritative; update branch status and capability summaries whenever functionality is integrated.

## Suggested sequence

Waveform parsing and CLI simulation → finish M1 and model elaboration →
SPICE-compatible transient → nonlinear devices → remaining M5 usability. Numerical optimization is follow-up,
not grounds to weaken correctness gates.

## Initially out of scope

Advanced BSIM-family models, XSPICE code models, OSDI/Verilog-A, CIDER, Tcl,
the full numparam compatibility surface and the interactive command interpreter.
Basic `.param` evaluation is still required by M1.

## Development and publication

Implement and test in the selected Rust development worktree; leave sibling
checkouts and upstream C sources unchanged. C references are read-only and the C
binary is used out of process. Do not copy LGPL KLU algorithms into this BSD port.

The optional C-reference development checkout provides
`scripts/publish-rust-only.sh` and its disposable local routing/export tests;
these scripts are not part of a standalone Rust-only clone. Synchronize the
lagging development mainline with the implemented solver work before doing any
whole-tree export, preserving newer documentation on both sides.

Commit or publish only when requested. Inspect source/target branches, histories
and cleanliness first: the development publisher maps `rust-port` to public
`main` and retains other named branches. Never push C-tree history into this
standalone repository or overwrite newer target-side work. Transfer focused
changes on top of the current public main when the source checkout is behind.
