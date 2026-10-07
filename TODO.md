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
| Parser | `.param`/`{expr}` syntax (#14) with top-level evaluation (#15), scalar R/C/L/V/I, declared-model passives, models, bounded D/Q/M flags/IC vectors, PULSE/PWL, scoped subcircuits/X and resolved includes/libraries; 8/8 fixture parses, not round trips or simulation |
| Model inputs | Top-level first-wins resolver, family/level checks, bounded passive factories and diode input schemas |
| Passive models | R sheet/C area-perimeter geometry, scalar model L, TC1/TC2/TEMP/TNOM, scale and multiplicity |
| Topology | Petgraph circuit incidence and matrix-row graphs, simulation branch-row binding |
| Linear equations | faer real/complex LU, scalar R/C/L/V/I elaboration and stamps |
| DC / AC | Linear `.op`, one-source `.dc`, complex RLC `.ac` |
| Transient | Explicit diffsol adaptive BDF for index-one DAEs, including floating/coupled capacitor mass blocks; higher-index pencils rejected |
| Nonlinear devices | D/Q/M syntax and initial diode input validation; no equations |
| CLI | Inspection/parsing; simulation through APIs/examples only |

**No full M1/M3 completion or full SPICE parity is claimed.** The implemented BDF
is not ngspice trapezoidal or fixed Gear-2. Its Step/Pwl source waveforms are
available through the device API. Numeric PULSE/PWL deck syntax now parses (#8),
but waveform deck elaboration/evaluation is still unavailable; higher-index constraints,
nonlinear charge, `.ic` and `uic` remain unsupported. Floating/coupled capacitor
index-one DAEs are supported by the BDF backend (#28).

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

- [x] Parse bounded D/Q/M bare OFF and model-family tail flags (#10); thermal/sensitivity/CIDER, extra ports, binning and advanced forms remain gaps. See [FRONTEND_VALUES.md](docs/port/FRONTEND_VALUES.md).
- [x] Parse Q 1–2/M 1–3 value IC vectors (#10), with positioned components and ordered duplicates/scalar setters; no initialization support.
- [x] Parse declared-model R/C/L without mistaking models for parameter references (#11); forward references, omitted values and bounded geometry-only forms, not arithmetic.
- [x] Represent numeric PULSE (2–7 fields) and bounded paired PWL (#8), retaining timing omissions and DC/AC application order; factories reject unimplemented runtime semantics.
- [x] Pin malformed/unsupported variants, byte positions, atomic factory errors and live C setter/coefficients probes; enable transient fixture AST/CLI parse tests (seven fixture parses, not full M1).

### M1c: subcircuit and file structure

- [x] Parse `.subckt`/`.ends` ports, unevaluated ordered formal parameters, nested scope storage and X instances (#12).
- [x] Diagnose unmatched/mismatched/missing terminators and scope-local duplicate definitions; preserve ordered cards and bounded nested bodies.
- [x] Parse `.include`/`.inc`/`.lib` paths and sections, preserving quoted spelling and selected library boundaries (#13).
- [x] Implement source-relative resolution, canonical file/section cycle checks, depth/file/byte/card limits and include-chain/source provenance.
- [x] Use a directed petgraph file/section dependency graph with incremental reachability checks; no custom graph engine.
- [x] Parameter evaluation (#15) uses a directed petgraph dependency graph (cycles via SCC, order via toposort levels).
- [ ] Use directed petgraph dependency graphs for subcircuit elaboration (#18, M5).
- [x] Enable `subckt_divider` AST/CLI tests: 8/8 fixture parsing, not flattening or the full M1 gate. See [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md).

### M1d: parameter semantics and full front-end gate

- [x] Parse `.param` cards and a bounded numparam expression grammar (#14): winnow precedence, C-pinned `^`/sign rules, function allowlist, braced values at device/model/analysis/X sites, positioned unevaluated AST. See [PARAM_EXPRESSIONS.md](docs/port/PARAM_EXPRESSIONS.md); no evaluation.
- [x] Evaluate top-level `.param` values (#15): C-backed order/redefinition/forward-reference rules, petgraph cycle and undefined-name chains, bounded work, finite-or-error arithmetic (`spice_netlist::eval`), and a literalized netlist copy consumed by `Circuit` elaboration and `RunConfig::request_for` (`spice_netlist::elaborate`). Opt-in C probes: `c_param_eval`. See [PARAM_EXPRESSIONS.md](docs/port/PARAM_EXPRESSIONS.md).
- [ ] Subcircuit formal defaults/overrides and scoped evaluation (scope API is ready; subcircuits still rejected), `{expr}` option values, quoted/`'expr'` values, `.func`.
- [x] Parse `.option` and `.global` (#16): ordered positioned settings, `spice_analysis::RunConfig` for temp/tnom/reltol/vntol/abstol (method/maxord retained, rejected for `.tran`; all other options error). Top-level `.global` contract for a future flattener; body-local `.option`/`.global` and flattening remain pending. See [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md).
- [x] Add normalized-deck serialization preserving source parameter application order (#20): `spice_netlist::write_netlist` plus location-free `semantic_eq`/`semantic_diff` (contract in [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md)); directives are written, resolved include content is not inlined. The eight-fixture round-trip *gate* (#22) remains open.
- [x] Commit deterministic token/AST snapshots and document regeneration (#21): `spice_netlist::dump`, 62 files under `conformance/snapshots/`, `cargo xtask snapshots [--bless]` (see `conformance/snapshots/README.md`). The eight-fixture gate (#22) remains.
- [ ] Round-trip all eight rawfile fixture decks through AST and normalized text (#22).
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

- [x] Implement trapezoidal and Gear orders 1–2 coefficients, companion integration, prediction and per-element truncation estimates (`src/maths/ni/`, `cktterr.c`) with trial coefficients separate from accepted step history (#23); Gear orders 3–6 are rejected.
- [ ] Implement adaptive timestep scheduling and truncation-error control in a companion transient driver (#26).
- [x] Implement C/L trap/Gear-2 companion stamps from accepted charge/flux state without double-discretizing diffsol equation stamps (#25); sources still stamp only at DC, mutual inductance and companion `ic=`/`uic` are pending.
- [ ] Implement `.ic`, `.nodeset`, instance IC and `uic` semantics with consistent constraints/derivatives.
- [ ] Connect parsed source waveforms to time evaluation and breakpoint handling, including `PULSE`.
- [x] Demonstrate an index-one formulation for floating/coupled capacitor networks in the BDF backend: block-SVD `ker E`/`ker Eᵀ`, rank-certified `Wᵀ A N`, charge-preserving event projection and consistent derivatives, with analytic and opt-in C tests (#28).
- [ ] Design and validate bounded higher-index source-constraint support (#29); such pencils remain rejected.
- [x] Define explicit trial-versus-accepted device state: `&self` trial loads into a disposable `TrialState`, rotating `StateHistory`, per-device branch/state ranges and integration context, atomic accept hooks before commit (#24).
- [ ] Retain accepted-state/trial-state separation in every transient/Newton driver, event left/right limits, voltage/current tolerances, output-grid separation and progress/work budgets.
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

#12/#13 scoped/source syntax → #14–16 expressions/evaluation/options/globals →
#20–22 serialization/snapshots/full M1 round-trip gate. Keep #18 flattening in M5.
CLI simulation and model elaboration →
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
