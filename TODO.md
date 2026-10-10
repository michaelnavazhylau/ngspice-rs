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

The consolidated single-crate layout is unmerged work on
`restructure/single-crate`, based on `9832167`: the six port crates are now the
module tree under `src/` in one package, `ngspice-rs`. `main` still carries the
six-crate `crates/spice-*` layout, so an export from a checkout that has not been
updated would revert it — treat the layout change as whole-tree, never as a
cherry-pick. Layout, manifests and documentation changed only; no simulation path
did. Validation on that tree: **1051 passed / 0 failed / 67 ignored** on stable
and Rust 1.89.0 (`ngspice-rs` alone **1003/0/67**; the delta is `xtask`), `golden
verify` **59/0/0**, **196** snapshots unchanged, and the packaged `.crate` passes
its own suite (**1003/0/67**). See
[VERIFICATION.md](docs/port/VERIFICATION.md).

PR #51 (`cdc078c`) merged #11 passive syntax and #17 initial model infrastructure;
their acceptance suite passes on this main-based branch. This checkout implements
bounded passive elaboration (#19), pending merge. Completed entries describe the
checkout, not a claim that all M2 slices have already merged.

| Area | Current checkout |
| --- | --- |
| Parser | `.param`/`{expr}` syntax (#14) with top-level evaluation (#15), scalar R/C/L/V/I, declared-model passives, models, bounded D/Q/M flags/IC vectors, PULSE/PWL (elaborated to forcing, #9), scoped subcircuits/X and resolved includes/libraries; 8/8 fixture parses and normalized round trips (M1 gate, #22); not simulation |
| Model inputs | Top-level first-wins resolver, family/level checks, bounded passive factories and diode input schemas |
| Passive models | R sheet/C area-perimeter geometry, scalar model L, TC1/TC2/TEMP/TNOM, scale and multiplicity |
| Topology | Petgraph circuit incidence and matrix-row graphs, simulation branch-row binding |
| Linear equations | faer real/complex LU, scalar R/C/L/V/I elaboration and stamps; opt-in library equilibration wrappers, original-unit residuals and unchanged defaults (#46, [EQUILIBRATION.md](docs/port/EQUILIBRATION.md)) |
| Numerical follow-up | #47 backend/rank audit retains production guards (no enabled optimization or formal certificate; proof follow-up #68); #29 constrained-RLC prototype/ADR is not production-enabled; runtime gates #69–#72 |
| DC / AC | Linear and bounded nonlinear `.op`, typed/nested source/resistor/temperature/`@inst[param]` `.dc` (#97), bias-linearized `.ac` and `.pz` (#103) |
| Transient | Ordinary `.tran`: adaptive trapezoidal / Gear-2 companion driver with truncation-error control and breakpoint landing (#26, [TRANSIENT.md](docs/port/TRANSIENT.md)), linear and bounded nonlinear charge paths, `.ic`/`uic`/`.nodeset` and D/Q/M `off`/`ic=` for linear and nonlinear circuits (#27, #99); explicit `backend=diffsol method=bdf` adaptive BDF for index-one DAEs, including floating/coupled capacitor mass blocks; higher-index pencils rejected |
| Nonlinear devices | Bounded diode/MOS1 and Gummel-Poon level-1 BJT (#87) DC/AC/charge-companion paths; [M4 support/gate](docs/port/M4_NONLINEAR.md) |
| CLI | Inspection/parsing plus `spice-rs simulate --output <path> <deck>` (#6): every `.op`/`.dc`/`.ac`/`.tran` card through the production runner in ngspice batch order with one plot per analysis (#96), per-analysis `.save`/`.print`/`.measure`/`.four`, atomic ASCII rawfile written via temporary file plus rename; exits 0/1/2/3. See [CLI.md](docs/port/CLI.md) |
| Output selection | `.save`/`.print` cards project the full plot into the written rawfile in C `dbs` order with first-wins dedup and bounded operand support (`v(n)`, `v(n1,n2)`, `i(source|inductor|E|H)`, `vm`/`vp`/`vr`/`vi`/`vdb`); `.print` also renders a text table; unsupported/unresolvable requests fail before publishing; `.plot` unported (#42, [OUTPUT_SELECTION.md](docs/port/OUTPUT_SELECTION.md)) |
| Measurements | `.measure`/`.meas` bounded subset (`FIND … AT=`, `MIN`/`MAX`/`AVG`/`RMS`/`INTEG`, `TRIG … TARG …`) evaluated over the full plot before output selection narrows it; a failing card fails the run; the remaining variants are unported (#43, [MEASURE.md](docs/port/MEASURE.md)) |
| Fourier | Bounded `.four` (#44): final complete transient period, physical-grid resampling, DC, single-sided peak amplitudes, window-referenced phase in radians and THD; 1–100 harmonics; full-plot evaluation before output selection ([FOURIER.md](docs/port/FOURIER.md)) |

**No full M1/M3 completion or full SPICE parity is claimed.** The implemented BDF
is not ngspice trapezoidal or fixed Gear-2. Numeric PULSE/PWL V/I
setters (#8) elaborate to analytic Pulse/Pwl forcing (#9) with lazy breakpoints
and left/right limits (Step is device-API only). The BDF backend still rejects
higher-index constraints, nonlinear charge, `.ic` and `uic`; #29's numeric
prototype does not widen it. Floating/coupled capacitor index-one DAEs are
supported by that backend (#28).

**M5's bounded scope is complete.** Wave 1 merged subcircuit instantiation
(#18, [SUBCIRCUITS.md](docs/port/SUBCIRCUITS.md)), the `spice-rs simulate`
command (#6, [CLI.md](docs/port/CLI.md)) and binary rawfile read/write (#45,
[RAWFILES.md](docs/port/RAWFILES.md)). Wave 2 merged bounded `.save`/`.print`
output selection (#42, [OUTPUT_SELECTION.md](docs/port/OUTPUT_SELECTION.md)) and
bounded `.measure`/`.meas` measurements (#43,
[MEASURE.md](docs/port/MEASURE.md)). Final-period `.four` (#44,
[FOURIER.md](docs/port/FOURIER.md)) completes the six deliverables; `.plot` and
extended output/measurement/Fourier forms remain unported.

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
- [x] Consolidate the six port crates into one publishable package, `ngspice-rs` (`restructure/single-crate`): sources now live under `src/primitives`, `src/netlist`, `src/maths`, `src/devices`, `src/analysis` and `src/cli`; that layering is a review-enforced convention rather than a compiler-enforced one. The binary is `src/bin/spice-rs.rs`, integration tests are `tests/*.rs` at the package root and `xtask` stays an unpublished workspace member. Conformance fixtures are now package-relative (`../conformance/...` from `tests/`), so a published `.crate` can run its own test suite. No functional change; see [VERIFICATION.md](docs/port/VERIFICATION.md).

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

- [x] Parse bounded D/Q/M bare OFF and model-family tail flags (#10); thermal/sensitivity/CIDER, extra ports and advanced forms remain gaps (binned M references parse since #109). See [FRONTEND_VALUES.md](docs/port/FRONTEND_VALUES.md).
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
- [x] Use directed petgraph dependency graphs for subcircuit elaboration (#18, M5): `expand_subcircuits` builds the reachable definition graph and rejects recursion via its SCCs. See [SUBCIRCUITS.md](docs/port/SUBCIRCUITS.md).
- [x] Enable `subckt_divider` AST/CLI tests: all eight fixtures parse (the M1 round-trip gate is #22). With #18 the deck also simulates through the production `.op` path and is verified against its C golden. See [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md).

### M1d: parameter semantics and full front-end gate

- [x] Parse `.param` cards and a bounded numparam expression grammar (#14): winnow precedence, C-pinned `^`/sign rules, function allowlist, braced values at device/model/analysis/X sites, positioned unevaluated AST. See [PARAM_EXPRESSIONS.md](docs/port/PARAM_EXPRESSIONS.md); no evaluation.
- [x] Evaluate top-level `.param` values (#15): C-backed order/redefinition/forward-reference rules, petgraph cycle and undefined-name chains, bounded work, finite-or-error arithmetic (`netlist::eval`), and a literalized netlist copy consumed by `Circuit` elaboration and `RunConfig::request_for` (`netlist::elaborate`). Opt-in C probes: `c_param_eval`. See [PARAM_EXPRESSIONS.md](docs/port/PARAM_EXPRESSIONS.md).
- [x] Subcircuit formal defaults/overrides and scoped evaluation (#18, M5): `X` instances expand through the production entry points with hierarchical names, per-instance parameter/model scope and `.global`; precedence is instance override > body `.param` > formal default. See [SUBCIRCUITS.md](docs/port/SUBCIRCUITS.md).
- [x] Quoted `'expr'` values and `.func` definitions (#107): single quotes delimit an expression wherever braces do (C `inp_change_quotes()`); `.func` cards (and the `.param f(x)=body` spelling C rewrites to `.func`) at the root and in `.subckt` bodies with lexical scoping, last-definition-wins, built-in override, by-value calls with call-site free names, petgraph recursion detection and arity checks (a `.subckt` body's only when instantiated, as in C; `v()`/`i()` device-value calls are `NotYetPorted`) (`netlist::eval::FunctionScope`, `ParamScope::for_netlist`). Opt-in C probes: `c_func_eval`; golden `func_quotes`. See [PARAM_EXPRESSIONS.md](docs/port/PARAM_EXPRESSIONS.md).
- [x] `{expr}` and `'expr'` option values (#107, option part): parsed into `OptionSetting::expression`, evaluated by `RunConfig::from_netlist` against top-level `.param`, writer/semantic round trip, opt-in C probe (`c_options_reference`). See [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md).
- [x] Broaden `.option` coverage (#110): junction `gmin`, `itl2` (continuation stage limit of every DC bias, `.dc` warm starts), `itl4`, `itl6`, `xmu`, DC options for the companion `.tran` initial bias, documented no-ops (print flags, C-ignored `itl3`/`itl5`/..., `post`/`ingold`, `bypass=0`) with `RunConfig::ignored`; `itl1`/`itl2`/`itl4` follow C's effective `max(n, 100)` (`niiter.c`), BJT gmin scales with `m`; the rest (incl. `pivtol`/`pivrel`) stay `NotYetPorted`. Opt-in C comparisons in `c_options_reference`, goldens `options_gmin_dc`/`options_xmu_tran`. Status table in [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md#option-coverage-110). Still pending: `pivtol`/`pivrel` (no LU pivot-threshold knob), `gshunt`/`cshunt`/`rshunt`, `noopiter`, `minbreak`, `numdgt`, front-end output variables; `.option gmin` does not yet seed the DC gmin-stepping ladder, and C's adaptive `dynamic_gmin`/`gillespie_src` step control (raw `itl2/4`) has no counterpart in the fixed ladders.
- [x] Parse `.option` and `.global` (#16): ordered positioned settings, `analysis::RunConfig` for temp/tnom/reltol/vntol/abstol (method/maxord retained, rejected for `.tran`; all other options error). Top-level `.global` contract consumed by the #18 flattener; `.option`/`.global`/`.ic`/`.nodeset` inside a body remain rejected. See [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md).
- [x] Add normalized-deck serialization preserving source parameter application order (#20): `netlist::write_netlist` plus location-free `semantic_eq`/`semantic_diff` (contract in [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md)); directives are written, resolved include content is not inlined. The eight-fixture gate is closed by #22 below.
- [x] Commit deterministic token/AST snapshots and document regeneration (#21): `netlist::dump`, 66 files under `conformance/snapshots/`, `cargo xtask snapshots [--bless]` (see `conformance/snapshots/README.md`).
- [x] Close the M1 front-end fixture and round-trip gate (#22): `tests/m1_gate.rs` pins per-deck device/terminal/model/analysis expectations for all eight `conformance/netlists` decks, parse→write→parse semantic equality with a writer fixed point, matching committed token/AST snapshots, combined fixtures `conformance/parser/combined_{includes,params_options}.cir` (includes + subcircuit + params + options; include provenance and setter order), and negative coverage (malformed cards, unsupported forms, include/parameter cycles, scoped-name containment). `tests/m1_combined.rs` runs the params+options fixture through `RunConfig`. Scoped-name enforcement is structural only: declarations stay in their scope and Q-family lookup sees only the local scope and ancestors; unresolved X targets and D/M model names are *not* resolved at parse time (elaboration owns that).
- [x] `cargo xtask ci` is green with the gate; the M1 front-end gate is closed. **Not part of this claim and still outstanding:** `{expr}` option values, quoted/`'expr'` values, `.func`, directives inside a body (`.option`/`.global`/`.ic`/`.nodeset`), scoped model resolution/binning (quoted values and `.func` later landed with #107). D/Q/M decks parse only; this is not nonlinear simulation parity. (#18 later added subcircuit elaboration on top of the gate; see [SUBCIRCUITS.md](docs/port/SUBCIRCUITS.md).) C parser oracles remain opt-in (`NGSPICE_BIN`).

## 3. Model elaboration and validation

- [x] Top-level first-declaration model lookup, family compatibility and family-specific level selection/rounding (#17); failed elaboration leaves circuit state unchanged.
- [x] Device-owned scalar schema extension API and bounded diode IS/N/RS/AREA/TEMP/TNOM defaults/ranges (#17); raw AST preserved; M4 adds bounded model-aware D/Q/M factories (see its support table).
- [x] Scoped model resolution (#18) and MOS model binning (#109): `devices::binning` ports `INPgetModBin`/`model_name_match` (inclusive 1 nm edges, last declared match wins, `nf`/`wnflag`/`scale` inputs) with subcircuit-scoped bin sets, checked against C by `tests/c_binning_reference.rs`. C bins only BSIM3/BSIM4/HiSIM, so a selected bin stays `NotYetPorted` until those families land; `.options scale`/`wnflag` and instance `nf` are not deck inputs yet. See [MODEL_SCHEMAS.md](docs/port/MODEL_SCHEMAS.md#model-binning-109).
- [ ] Add further device-owned schemas/defaults; expand only with production tests.
- [x] Preserve omitted versus explicit BJT substrate terminals; bounded M4 factories reject unavailable substrate physics/backends.
- [x] Define and implement the bounded passive value/geometry/temperature surface (#19); explicit errors for unsupported setters, missing/invalid geometry and nonfinite derivations.
- [ ] Expand passive aliases, coil geometry, DTEMP/TCE/AC-only values and other advanced forms only with documented formulas and conformance tests.

## 4. Complete SPICE-compatible transient analysis — M3

Complex linear AC is already implemented on main. Bounded adaptive
BDF does not close the following trap/Gear and general-transient requirements.

- [x] Implement trapezoidal and Gear orders 1–2 coefficients, companion integration, prediction and per-element truncation estimates (`src/maths/ni/`, `cktterr.c`) with trial coefficients separate from accepted step history (#23). Gear orders 3–6 (#98): variable-step coefficients, predictors and `CKTterr` estimates with six accepted steps and seven state vectors, tested for polynomial exactness, the BDF tables, an independent Vandermonde solve and fixed-step convergence order; reachable through `maths::integrator` only, because `dctran.c` never raises the order above 2.
- [x] Implement adaptive timestep scheduling and truncation-error control in a companion transient driver (#26): `analysis::companion_transient` / ordinary `.tran`, trap and Gear-2 (`maxord` 1-6 for both methods; like `dctran.c` only 1 versus more than 1 matters, #98, golden `rlc_series_gear_maxord6_tran`), `dctran.c` step/order/breakpoint policy, accepted points as output, work/min-step limits; analytic RC/RL/RLC and PULSE tests, `rc_transient` registered in `golden verify`, opt-in live C agreement ([TRANSIENT.md](docs/port/TRANSIENT.md)). Linear circuits only; Newton structure is in place for M4.
- [x] Implement C/L trap/Gear-2 companion stamps from accepted charge/flux state without double-discretizing diffsol equation stamps (#25); independent sources stamp their time-`t` forcing (left/right limit) in companion loads (#26); mutual inductance landed with #80 (`ic=`/`uic` landed with #27).
- [x] Implement `.ic`, `.nodeset`, instance IC and `uic` semantics with consistent constraints (#27): `.ic`/`.nodeset`/`uic` flow from `RunConfig` into every `AnalysisRequest`; the companion `.tran` enforces `.ic` as exact row constraints in the initial bias (ideal-source conflicts are errors), starts `uic` runs from capacitor/inductor `ic=` charges and fluxes with an exact impulse-freedom check, treats `.nodeset` as a validated no-op for linear circuits (C quirk: used as node IC under `uic`), validates nodes in every analysis, and keeps `backend=diffsol` explicit-reject; analytic RC/RL/RLC tests and opt-in live C agreement ([TRANSIENT.md](docs/port/TRANSIENT.md)). Coupled inductors landed with #80; nonlinear initialization with #99 (section 9).
- [x] Connect parsed PULSE/PWL source waveforms to time evaluation and breakpoint handling (#9): validated analytic `Pulse` with C defaults resolved from the `.tran` step/stop (`PulseSpec`/`TransientTiming`), `Waveform::value_at(t, Limit)` and lazy `breakpoints_in(t0, t1)`; diffsol BDF stops at every corner/jump (budget 100k segments) and starts from the t=0 forcing; the companion driver (#26) lands on every breakpoint lazily. PULSE pulse count, PWL `r=`/`td=` and SIN/EXP/SFFM/AM followed in M6 (#94, #95, next item).
- [x] M6 source functions (#94, #95): positioned `SIN`/`SINE`/`EXP`/`SFFM`/`AM` syntax (`SourceWaveform::Function`), the PULSE eighth field (pulse count in ngspice's default compatibility mode) and PWL `td=`/`r=` as ordered scalar setters, for V and I sources; writer round trips. `devices::functions` resolves C's per-field defaults from `.tran` (`FunctionSpec::resolve`), evaluates analytic values with left/right limits, enumerates lazy corners (the port also lands on SIN/SFFM/AM delays and EXP `TD1`/`TD2`, where C sets no breakpoint) and gives OP/DC the C time-zero value (SFFM/AM: 0). Five C goldens (`rc_sin_tran`, `rc_exp_tran`, `rc_sffm_am_tran`, `rc_pwl_repeat_tran`, `rc_pulse_count_tran`) verify under `compare::TRAN`; repeated-PWL boundaries are left/right jumps in both backends (C's single boundary value depends on ulp-level landing; the measured divergence, and that of the port's extra SIN/EXP/SFFM/AM/fractional-PULSE breakpoints, is bounded by an opt-in unmarked-deck comparison); a `.four` analytic spectrum/THD test on a SIN-driven RC; opt-in `c_source_functions` compares 22 sources' values with C at C's own timepoints, `.op` time-zero values and a SIN-driven clipper's THD. See [TRANSIENT.md](docs/port/TRANSIENT.md#source-functions-94-95). The diffsol BDF backend rejects SIN/EXP/SFFM/AM explicitly (it interpolates forcing linearly between breakpoints). Still unported: TRNOISE/TRRANDOM/EXTERNAL, PWL `file=`, expression-valued waveform fields, negative delays, ngspice's `xs` compatibility (PULSE phase).
- [x] Demonstrate an index-one formulation for floating/coupled capacitor networks in the BDF backend: block-SVD `ker E`/`ker Eᵀ`, rank-checked `Wᵀ A N`, charge-preserving event projection and consistent derivatives, with analytic and opt-in C tests (#28).
- [x] Deliver the bounded higher-index formulation/prototype gate (#29): fallible fully voltage-constrained RLC reduction/reconstruction, original/differentiated residuals, IC/rank/impulse refusal and analytic tests; production pencils remain rejected. See [HIGHER_INDEX_DAE_ADR.md](docs/port/HIGHER_INDEX_DAE_ADR.md); runtime enabling is separately gated by #69–#72.
- [x] Define explicit trial-versus-accepted device state: `&self` trial loads into a disposable `TrialState`, rotating `StateHistory`, per-device branch/state ranges and integration context, atomic accept hooks before commit (#24).
- [x] Accepted-state/trial-state separation, event left/right limits, distinct voltage/current tolerances and progress/work budgets in the companion driver (#26); tests show rejected trials never advance histories and accept-hook failures abort the run. Every future Newton/nonlinear driver must keep this.
- [x] Complete RC/RLC transient and AC C-golden exit gates on common physical sample grids; do not require identical adaptive timesteps (#48). `xtask/src/tran.rs` event-aware comparator with `compare::TRAN`; twelve fixtures registered in `golden verify` at that point, sixteen with the initialized-state fixtures below (8 new: RL/RC-Gear/RC-PWL/RLC trap+Gear/floating/coupled `.tran`, RLC `.ac`), with Rust-only explicit-BDF variants against the same goldens (`TRAN`, or the peak-scaled `TRAN_RESTART` where C's backward-Euler restart error exceeds the pointwise bound); analytic closed-form, KCL, charge, energy and production-API gate tests in `tests/m3_gate.rs` ([VERIFICATION.md](docs/port/VERIFICATION.md)).
- [x] Initialized-state (`.ic`/`uic`/instance `ic=`) conformance fixtures (#48 remainder): `rc_ic_uic_tran` (RC discharge from `ic=2`), `rlc_ic_uic_tran` (series RLC from inductor `ic=20m` and capacitor `ic=1`), `rc_ic_node_tran` (`.ic v(out)=0.25` without `uic`: constrained initial bias, then released) and `floating_cap_ic_tran` (floating capacitor with a 2 uC plate charge). New C goldens captured one at a time with `cargo xtask golden capture --netlist <name>` (no existing golden touched), registered in `golden verify` with `compare::TRAN` unchanged (16 verified, worst error 0.000 of the bound); no BDF variants because the diffsol backend rejects `.ic`/`uic`/`ic=` (a test asserts the rejection). The comparator's `tran::Grid` gained a `start` for `uic` runs (C writes no `t = 0` row; both plots must begin at the same first accepted step). Closed-form decay, conserved plate charge, `uic` first-row/step-breakpoint and `.ic`-release checks in `tests/m3_gate.rs`.
- Still blocked / unsupported by the gate (do not claim M3 or universal MNA DAE support): runtime higher-index source constraints (#69–#72), nonlinear initialization and physics beyond the M4 support table, integration above order 2 (ngspice has no such order policy: `maxord` 3–6 run as 2, #98).

## 5. Nonlinear devices and convergence — M4

Bounded M4 baseline merged in PR #58. Local `work/m4-followups` adds #34/#35
acceptance work; it has not been published and those issues remain open.
Exact schemas, physics exclusions and #41 evidence: [M4_NONLINEAR.md](docs/port/M4_NONLINEAR.md).

- [x] Reuse typed first-declaration model resolution and device-owned ordered schemas/defaults; extend bounded diode/BJT/MOS1 factories atomically.
- [x] Implement diode and Ebers-Moll BJT junction/charge equations and bounded MOS1 square-law/body/overlap charge with analytic Jacobian stamps.
- [x] Add reusable Newton iteration with global voltage-step damping, physical residual checks, source/nodal-gmin stepping and request/deck physical tolerances. (Superseded as the default by ngspice's device limiting and `CKTop` schedules in #106, section 9; the global damping and ladders remain as `limiting=global`/`continuation=ladder`.)
- [x] Add typed independent-source/temperature/scalar-resistor DC targets and one nested outer axis; preserve originals, validate reachable grids, rebuild R/TEMP operators, retain source-only linear LU reuse (#35 local follow-up; [DC_SWEEPS.md](docs/port/DC_SWEEPS.md)).
- [x] Add bounded configurable DC continuation schedules/budgets, success/failure stage reports, and request > deck > default controls shared by OP/DC/AC bias (#34 local follow-up; [DC_CONTINUATION.md](docs/port/DC_CONTINUATION.md)); explicit continuation controls reject for transient.
- [x] Implement bias-linearized AC and actual nonlinear Q-based trap/Gear-2 companions, multi-charge LTE and disposable trial/atomic accepted history.
- [x] Demonstrate diode DC, BJT bias and MOS1 operating point against existing C data; add six C AC/charge-transient decks, physical/Jacobian/conservation/continuation checks and explicit tolerances for local #41 subset.
- [x] Reject unimplemented parsed physics; BSIM/CIDER/XSPICE and full SPICE parity remain outside scope.
- [x] Gummel-Poon level-1 BJT (#87): `devices::bjt` with base charge `qb` (VAF/VAR/IKF/IKR/NKF), ISE/ISC leakage, IBE/IBC, RB/RBM/IRB, RC/RE internal nodes, XTF/VTF/ITF transit time, XCJC, substrate ISS/CJS with SUBS, full `bjttemp.c` temperature/area scaling (TLEV 0/1/3, TLEVC 0/1, polynomial coefficients, AREAB/AREAC, DTEMP); exact Newton/companion Jacobians, `bjtacld.c` small signal. C goldens `m7_bjt_gummel`/`m7_bjt_output` (nested; verify drops Rust's outer `sweep(...)` column)/`m7_bjt_temp` (verify maps C's `temp-sweep` scale)/`m7_bjt_amp_ac`/`m7_bjt_amp_tran`, FD-Jacobian/conservation tests and opt-in `c_bjt_reference`. Still `NotYetPorted`: excess phase (PTF with TF), quasi-saturation (RCO...), KF/AF noise, SOA limits. OFF/IC landed with #99 (section 9). `DEVpnjlim` limiting landed with #106 (section 9). See [M4_NONLINEAR.md](docs/port/M4_NONLINEAR.md#bjt).
- [x] Complete MOS1 (#88, `devices::mos1`): Meyer gate charge with C's state-averaging formulation (nonzero TOX, CGSO/CGDO/CGBO), RD/RS/RSH+NRD/NRS internal drain/source nodes, AD/AS/PD/PS junction geometry with CJ/MJ/CJSW/MJSW/JS, TOX/UO/NSUB/TPG/NSS process extraction, LD, `mos1temp.c` temperature scaling (TEMP/DTEMP/TNOM), forward body bias with GAMMA > 0, `mos1acld.c` AC and `mos1trun.c` truncation (gate charges only). Four C goldens (CMOS inverter and 3-stage ring-oscillator transients, Meyer AC at 75 C, process/temperature DC) plus charge and finite-difference Jacobian tests; see [M4_NONLINEAR.md](docs/port/M4_NONLINEAR.md#mos1). MOS1 OFF/IC/ICVDS/ICVGS/ICVBS landed with #99 (section 9); noise parameters remain `NotYetPorted`.
- [ ] Expand beyond this demonstrated subset only with new production conformance: BJT excess phase and quasi-saturation, further `.dc @inst[param]` instance parameters (C has no model-setter sweeps; #97 landed the diode/BJT/MOS1/E-F-G-H subset, section 10) (nonlinear `.ic`/`uic` landed with #99, section 9). (C's convergence algorithm landed with #106, section 9; `OPtran`, predictor/bypass and `gshunt` remain.)

## 6. Usability and output — M5

M5 waves 1 and 2 are merged: #18 (subcircuits), #6 (`simulate`), #45 (binary
rawfiles), #42 (`.save`/`.print` output selection) and #43 (`.measure`).
Final-period `.four` (#44) completes the bounded milestone; documented exclusions
remain explicit, rather than implying complete ngspice output compatibility.

- [x] Add a CLI simulation command (#6) that elaborates supported decks, runs one analysis and writes an ASCII rawfile while preserving exits 0/1/2/3. See [CLI.md](docs/port/CLI.md).
- [x] Implement subcircuit instantiation/flattening, parameter passing, model/node scoping and `.global` semantics (#18). See [SUBCIRCUITS.md](docs/port/SUBCIRCUITS.md).
- [x] Add bounded `.print`/`.save` output selection (#42): typed positioned requests, C `dbs`-order projection of the full plot into the written rawfile with first-wins dedup, a `.print` text table and pre-publish failure for unsupported/unresolvable requests; `.plot` unported. See [OUTPUT_SELECTION.md](docs/port/OUTPUT_SELECTION.md).
- [x] Add bounded `.measure`/`.meas` measurements (#43): typed positioned requests (`FIND … AT=`, `MIN`/`MAX`/`AVG`/`RMS`/`INTEG`, `TRIG … TARG …`), evaluated over the full plot before output selection narrows the rawfile, with a failing card failing the run. See [MEASURE.md](docs/port/MEASURE.md).
- [x] Add bounded `.four` (#44): positioned frequency/vector requests, 1–100 harmonics, final-period physical-grid quadrature, DC/peak amplitude/window-referenced phase/THD, and atomic failure. Analytic, process and opt-in C gates pass. See [FOURIER.md](docs/port/FOURIER.md).
- [x] Add binary rawfile read/write (#45) so binary C rawfiles can be consumed; real/complex, explicit byte order, validated payload lengths. See [RAWFILES.md](docs/port/RAWFILES.md).
- [x] M6: run every analysis card of a deck (#96) in ngspice batch order (`CKTdoJob()`/`analInfo[]`: `.ac`, `.dc`, `.op`, `.tran`; same-type cards in reverse deck order) with C plot names, one plot each in a multi-plot rawfile; `.save` per plot, `.print` per analysis type, `.measure`/`.four` against the last plot of their type; atomic publication. Four-plot C golden `multi_analysis_rc` verified plot by plot (`BATCH` registry) plus an opt-in `ngspice -b -r` oracle. Interactive `run`/`resume` remains out of scope. See [CLI.md](docs/port/CLI.md#multi-analysis-decks).

## 7. Verification, numerical follow-up and documentation

- [x] Add `cargo xtask golden verify` for the three supported `.op`/`.ac` fixtures, comparing metadata and named real/complex components with the existing DC/AC bounds; explicit exclusions, failure diagnostics and process tests (GitHub #7).
- [ ] Expand production C comparisons as parser/device/analysis support lands; keep ordinary tests independent of a C toolchain.
- [ ] Preserve singular homogeneous/source-loop, disconnected valid block, mutation, pattern-change, finite/overflow and residual regression coverage.
- [x] Add opt-in library equilibration (#46): bounded positive power-of-two factors, dense/sparse/complex snapshots and physical-unit residual checks; defaults unchanged. See [EQUILIBRATION.md](docs/port/EQUILIBRATION.md).
- [x] Audit sparse rank-diagnostic cost (#47), retain unchanged production policy and document measured example-only complete-basis batching/guard regressions. No enabled optimization or formal uniqueness certificate. See [SPARSE_RANK_DIAGNOSTICS.md](docs/port/SPARSE_RANK_DIAGNOSTICS.md).
- [ ] Prove and independently review an aggregate sparse uniqueness certificate, including real/complex rounding/range bounds and changed rejection/overhead policy (#68); current guard's proof caveat remains unresolved.
- [ ] Gate higher-index runtime enabling separately: analytic waveform jets (#69), exact-class topology adapter (#70), reduced integration/reconstructed physical error control (#71), then nonimpulsive source corners (#72). No general DAE/default enablement follows from the prototype.
- [ ] Keep this checklist authoritative; update branch status and capability summaries whenever functionality is integrated.

## 8. Common-deck compatibility — M6

- [x] Linear controlled sources E/F/G/H (#78): winnow grammar for the C linear gain forms (parenthesized controls, HSPICE `vcvs`/`vccs`/`cccs`/`ccvs` keyword, leading versus named gain and G/F `m=` setter order), writer round trip and snapshots; `ControlledSource` stamps for OP/DC/AC/companion transient and the diffsol BDF assembly; F/H controlling sources (V, E, H) resolved to branch rows after elaboration, renamed hierarchically inside subcircuits, with positioned errors for unknown or non-findable controllers; E/H branch currents in plots, `.save`/`.print`/`.measure`/`.four`. C goldens `controlled_op`/`controlled_ac`/`controlled_tran` in `golden verify` (29 verified). LAPLACE remains explicit `NotYetPorted` (POLY/VALUE/TABLE and the implicit POLY(1) are lowered by #79, below); see [CONTROLLED_SOURCES.md](docs/port/CONTROLLED_SOURCES.md) for limits (AC conditioning with very high gains). `.ic` and `uic` load capacitors on E/H-driven nodes are reduced against the controlled-source relation and checked after the solve.
- [x] K mutual inductance (#80): winnow grammar for `Kname L1 L2 [L3 ...] k` (positional, `k=`, `coefficient=`, `{expr}`; every pair coupled as `inp_compat()` expands it), writer round trip and snapshots; inductor references renamed hierarchically in subcircuits and resolved after elaboration with positioned errors; `M = k sqrt(|L1 L2|)` from C's `INDinduct`; coupled flux in DC state, AC (`-j w M`), trap/Gear-2 companions, truncation and `uic`, and the diffsol BDF mass matrix; inductive systems that are not positive semidefinite (`|k| > 1`, inconsistent sets) are rejected where C only warns. C goldens `transformer_ac`/`transformer_tran`/`transformer_ic_uic_tran`/`transformer_model_uic_tran` in `golden verify`; model-backed C/L now join truncation control and apply instance `ic=` under `uic`; the definiteness check runs on the stamped (`L/m`) matrix and says when C stays silent; `indverbosity` is a documented no-op; registry C reference fixed (no longer `devices/cpl/`). See [MUTUAL_INDUCTANCE.md](docs/port/MUTUAL_INDUCTANCE.md).
- [x] Voltage/current-controlled switches S/W (#81): winnow `inp2s.c`/`inp2w.c` grammar with ordered `on`/`off` flags, `sw`/`csw` model cards, writer round trip and snapshots; `devices::switch` with C defaults (RON 1 ohm, ROFF = `gmin`), C's `swload.c`/`cswload.c` state rules per Newton phase (`IterationPhase`: trials carry C's `MODEINITF` phase and the previous iterate; `newton::solve_phased` never converges on a load that flipped a switch), accepted switch state committed only by `accept_point` (DC sweeps carry it from point to point, transients from the converged operating point), `swtrunc.c` step limits through `Device::timestep_limit` (W: none, as C), AC with C's `MODEINITSMSIG` state (the zero `CKTstate1`: open; `SWacLoad` counts every non-zero code as on); DC sweeps of discrete-state circuits visit C's accumulated values and nested inner sweeps restart from the flags (`dctrcurv.c` `firstTime`). C goldens `switch_op`/`switch_dc`/`switch_dc_decimal`/`switch_ac`/`switch_tran`/`switch_w_tran` in `golden verify` (44 verified, identical C timepoints); opt-in `c_switches`. The diffsol BDF backend, switch noise/pole-zero and `@s1[...]` queries remain unsupported, and C's non-nearest literal parsing (`0.7` read as `0.7000000000000001`) is an open parser-wide divergence; see [SWITCHES.md](docs/port/SWITCHES.md).
- [x] Behavioural sources (#79): B `v=`/`i=` expressions (C's `inpptree` grammar over the raw card text; braces/quotes transparent; 11-digit literal rounding; `.param`/`.func`/`=pwl(` numparam values resolved per scope) with the full `inpptree.c` function set, compiled to a value-and-gradient evaluator applying C's derivative rule per node (quirks included); `ASRCload` Newton stamps, `ASRCacLoad` AC Jacobian, `time`/`temper`/`hertz` (AC re-solves the bias per frequency, `CKTvarHertz`), `m`/`tc1`/`tc2`/`temp`/`dtemp`/reciprocal setters, `i()` of V/E/H/voltage-B branches; no breakpoints (as C); Newton's voltage-step damping skips behavioural outputs. E/G `VALUE`/`VOL`/`CUR`, `TABLE` (XSPICE `pwl` map), `POLY(n)` on E/G/F/H and the implicit `POLY(1)` are lowered like `inpcom.c`/`enhtrans.c` before subcircuit expansion, with `inp_meas_current()` current sensing. C goldens `bsource_op`/`bsource_dc`/`bsource_ac`/`bsource_tran`/`evalue_op`/`gtable_dc`/`epoly_dc` plus the zero-start `bsource_zero_op`/`bsource_zero_dc`/`bsource_zero_tran` in `golden verify` (58 verified with the M6 K and switch fixtures; XSPICE code models loaded via `* xtask-codemodels:`), FD-Jacobian tests for every function and an opt-in 1e-12 C value/derivative oracle (`c_behavioural_reference`). Newton retries a numerically failed linear solve with Curtis-Reid balancing and iterative refinement, so the `1e32` zero-start slopes of `1/x`/`sqrt`/`log` converge as in C. Still unported: `ddt`, the statistical functions `agauss`/`gauss`/`aunif`/`unif`/`limit` (`NotYetPorted`), infinite slopes at a Newton iterate (`v(x)^0.5` from 0 V stops where C continues through a NaN iterate), `LAPLACE`, compatibility-mode variants, user XSPICE cards, the BDF backend. See [BEHAVIOURAL_SOURCES.md](docs/port/BEHAVIOURAL_SOURCES.md).
- [x] `spice-rs devices`/`analyses` tables (#117): `DeviceSupport` (`ported`: built from the card; `bounded`: D/Q/M, S/W and X, built from a deck for a stated subset; `pending`) derived from the factory module's designator lists, pinned by `tests/registry_support.rs`, which elaborates one representative deck per designator; `.four` reported as a post-processor of `.tran` (`analysis::support`), op/dc/ac/tran no longer described as linear-only; K cites `devices/ind/mut*.c`. See [CLI.md](docs/port/CLI.md).
- [x] M6 exit gate: `m6_gate` (SIN-driven K transformer into a G/E op-amp subcircuit with `.param`/`.func`/`{expr}` values, a B limiter, an H sense and `.option reltol`) runs `.ac`/`.dc`/`.op`/`.tran` end to end; C golden verified plot by plot (`BATCH`, 59 verified) and circuit relations checked in `tests/simulate.rs`. See [VERIFICATION.md](docs/port/VERIFICATION.md#m6-exit-gate).

## 9. Nonlinear physics and convergence parity — M7

- [x] Diode physics (#86): reverse breakdown BV/IBV/NBV/TCV with `diotemp.c` matching; full `diotemp.c` temperature laws (EG/XTI, TLEV 0..2 with GAP1/GAP2, TLEVC 0/1 with CTA/CTP/TPB/TPHP, TM1/TM2, TTT1/TTT2, TRS/TRS2, DTEMP, TNOM); sidewall JSW/NS/CJSW/VJSW/MJSW/FCS with instance/model PJ and model AREA; ISR/NR recombination, JTUN/JTUNSW tunnelling, IKF/IKR/IKP knees; `dio.c` aliases; C's AC recombination conductance. C goldens `m7_zener_dc`/`m7_zener_tran`/`m7_diode_physics_dc`/`m7_diode_temp_dc`/`m7_diode_temp_ac`, FD Jacobian/charge unit tests per slice and closed-form production checks (`diode_physics.rs`). Still `NotYetPorted`: soft recovery (VP), RSW, self-heating, level 3 geometry, noise/SOA setters, common-characteristic sidewall breakdown, TM1/TM2 with CJSW. Breakdown matching uses C's default RELTOL. See [M4_NONLINEAR.md](docs/port/M4_NONLINEAR.md#diode).
- [x] Convergence parity (#106): `devices::limiting` ports `DEVpnjlim`/`DEVfetlim`/`DEVlimvds`, the `MODEINITJCT` start voltages and the `tVcrit` critical voltages, used by the diode (`dioload.c`, breakdown-reflected), Gummel-Poon BJT (`vbe`/`vbc`/`vsub`, `bjtload.c`) and MOS1 (`vgs`/`vgd` against `von`, `vds`, bulk junction, `mos1load.c`) loads, which linearize at the limited voltages and flag the load nonconvergent so a returned point is always an exact evaluation (no residual check weakened). Newton defaults to this device limiting with C's `itl1` = 100 (`limiting=global` keeps the 0.2 V damping). DC continuation defaults to ngspice's `CKTop`: `dynamic_gmin` then `new_gmin`, `gillespie_src`, or `spice3_gmin`/`spice3_src` for `gminsteps`/`srcsteps` > 1, adapted by the raw `itl2` (`continuation=ladder` keeps the fixed ladders); `.options noopiter`; `.dc` warm-starts every point at C's effective `itl2`. C goldens `m7_conv_latch_op` (metastable latch point), `m7_conv_latch_gillespie_op`/`_spice3_gmin_op`/`_spice3_src_op` (noopiter strategies), `m7_conv_latch_tran`, `m7_conv_bjt_schmitt` and `m7_conv_cmos_schmitt` (up/down hysteresis sweeps and an in-band `.op`) in `golden verify` (80 verified), plus `tests/convergence.rs`. Not ported: C's `OPtran` fallback (`optran.c`; an exhausted continuation error names it), the `MODEINITPRED` predictor and bypass, `gshunt`, `oldlimit`, `dyngmin`, BJT quasi-saturation limiting. See [DC_CONTINUATION.md](docs/port/DC_CONTINUATION.md) and [VERIFICATION.md](docs/port/VERIFICATION.md#m7-convergence-parity-106).
- [x] Nonlinear initial conditions (#99): `.ic` without `uic` forced as exact node-row constraints in every Newton load of the nonlinear transient operating point (all continuation stages, scaled by the source factor; `bias::NodeForcing`/`solve_dc_forced`), `.nodeset` forced only in the `MODEINITJCT`/`MODEINITFIX` loads of every nonlinear DC operating point (`.op`, `.ac`, `.dc` first points, transient bias) and then released, hints on source-fixed nodes dropped; `uic` performs C's single `MODETRANOP|MODEUIC|MODEINITJCT` load (`TrialState::with_initial_conditions`, `Linearization::InitialConditions`) with BJT `ICVBE`/`ICVCE` and MOS1 `ICVDS`/`ICVGS`/`ICVBS` defaults from the node vector (`bjtgetic.c`, `mos1ic.c`; the diode's `ic=` has no effect, as in C, whose setter never sets `DIOinitCondGiven`); D/Q/M `off` held at zero in `MODEINITJCT`/`MODEINITFIX` with C's device convergence test (`Limiter::holds_off`/`test_held`), MOS1 `ic=` as `MODEINITJCT` start voltages without `uic`; `off`/MOS1 starts rejected under `limiting=global`; BDF keeps rejecting `.ic`/`uic`. C goldens `m7_ic_diode_uic_tran`, `m7_ic_bjt_flipflop_tran`, `m7_ic_bjt_off_tran`, `m7_ic_mos1_uic_tran`, `m7_ic_latch_nodeset_op`, `m7_ic_latch_mos1_ic_op` (86 verified) and `tests/nonlinear_initial.rs`. Not reproduced: C's `1e10` compromise for `.ic`/`.nodeset` on source-fixed nodes (errors or dropped hints instead), `.op` seeding with `.ic` values; a `uic` vector that forward-biases a junction by volts can leave the first Jacobian too ill-conditioned for the rank-checked LU (an explicit numerical error). See [M4_NONLINEAR.md](docs/port/M4_NONLINEAR.md#nonlinear-initial-conditions-99), [TRANSIENT.md](docs/port/TRANSIENT.md#nonlinear-initial-conditions-99) and [VERIFICATION.md](docs/port/VERIFICATION.md#m7-nonlinear-initial-conditions-99).

## 10. Additional analyses — M8

- [x] Follow-up correctness fixes #128–131: bounded complex iterative refinement
  with unchanged rank/residual thresholds; registry-derived CLI diagnostic;
  shared pole-zero small-signal preparation; evaluated noise plot scheduling.
  See [verification](docs/port/VERIFICATION.md).
- [x] #132 implementation complete: RF noise covariances/two-port parameters;
  exact PORT accumulation on companion and bounded diffsol BDF backends,
  including missing DC/nonport cases and inherited source sweeps; remaining
  listed instance setters; SP measurements and retained bias plots; squared
  noise, named noise print vectors and SPICE3 MOS1 flicker. Live C comparisons
  cover hierarchical ports and every listed setter. See
  [verification](docs/port/VERIFICATION.md). GitHub issue state is unchanged.

- [x] `.tf` transfer function (#101): `analysis::tf` (`tfanal.c`, `dot_tf`) for `v(n)`, `v(n,m)` and `i(src)` outputs (any `CKTfndBranch` device) driven by independent V or I sources; the operating point as for `.op` (deck DC options, `.nodeset`), one reload of C's operating-point matrix (`TrialState::with_c_jacobian`: BJT `gx`-only base resistance as `bjtload.c`; switches with `swload.c` state rules) factored once for the unit-excitation and output-resistance solves, C's `1e20` open-resistance rule, the same-source copy and C's batch vector names (`v(Transfer_function)`, `v(<src>#Input_impedance)`, `v(output_impedance_at_V(n[,m]))`, `v(<src>#Output_impedance)`); batch order after `.tran`. C goldens `m8_tf_divider`/`m8_tf_controlled`/`m8_tf_bjt`/`m8_tf_batch` (90 verified), `tests/analysis_tf.rs` and opt-in `c_tf_reference`. Explicit errors where C carries on: unconverged operating point, unknown output node, `i(x)` without a findable branch, trailing tokens. See [TRANSFER_FUNCTION.md](docs/port/TRANSFER_FUNCTION.md).
- [x] DC sweeps over instance parameters (#97): C's `.dc` sweeps two levels of sources, resistors, `temp` and settable real *instance* parameters (`dctrcurv.c` `DCTfindInstParam`/`DCTsetInstParam`); it rejects model parameters and `.param` names and silently drops a third axis, which the port rejects. `analysis::sweep` resolves `@inst[param]` targets (`@v1[dc]`/`@i1[c]`/`@r1[r]` reuse the typed source/resistor routes); `Device::instance_parameter`/`with_instance_parameter` build immutable per-point replacements carried by `ModelContext::instance_overrides` for the diode (AREA/PJ/M/TEMP/DTEMP), BJT (AREA/AREAB/AREAC/M/TEMP/DTEMP, AREAB/AREAC kept as C's setup left them), MOS1 (M/L/W/AD/AS/PD/PS/NRD/NRS/TEMP/DTEMP) and E/F/G/H `gain` (G/F times a given `m`). The #132 real setters are also supported (R temperature/geometry/scale, C/L values, V/I AC and I multiplicity, D geometry/IC, Q/M IC components, G/F multiplicity, B scaling and K coupling); setup-only C effects are preserved. C goldens `m8_dc_param_diode`/`m8_dc_param_mos1`/`m8_dc_param_gain`/`m8_dc_res_temp` (90 verified), `tests/dc_parameter_sweeps.rs` and opt-in `c_dc_param_sweep_reference`. See [DC_SWEEPS.md](docs/port/DC_SWEEPS.md#instance-parameter-targets-97).
- [x] `.sp` S-parameter analysis (#105): RFSPICE port sources (`portnum`/`z0`/`pwr`/`freq`/`phase` in C setter order, `#res` internal node, series `z0` in every analysis, `vsrctemp.c` port numbering), `analysis::sparam` (`span.c`/`cktspdum.c`: unit port excitations, power waves, S/Y/Z with C's vector names and units, `v(Rbase)`, batch order after every other type) and `maths::dense_complex`. C goldens `sp_attenuator`/`sp_rc`/`sp_multi` in `golden verify`, analytic `tests/sparam.rs`, opt-in `c_sparam_reference`. The #132 follow-up adds `donoise` covariances/two-port parameters, exact PORT time forcing on both bounded backends, `.measure sp` and `keepopinfo`. Refused: AC current sources with an imaginary phasor in `.sp` (C copies it into every port solve). Nonexistent Y/Z blocks are zero (C's singular contract). See [SPARAM.md](docs/port/SPARAM.md).
- [x] Pole-zero analysis `.pz` (#103): `analysis::pz` reproduces C's card (`cur`/`vol`, `pol`/`zer`/`pz`), `PZinit` checks, the `vsrcpzld.c` removal of AC voltage sources, `CKTpzSetup`/`CKTpzLoad`'s drive and solution/balance-column modification and `PZpost`'s plot (`v(pole(k))`/`v(zero(k))`, conjugates after their root, empty plots flagged real); roots are the finite eigenvalues of the modified pencil by orthogonal staircase deflation of the infinite eigenvalues plus faer QZ (`maths::pencil`), not C's Muller search ([POLE_ZERO_ADR.md](docs/port/POLE_ZERO_ADR.md)). Opt-in `Device::assemble_pole_zero` for R/C/L/K/V/I/E/F/G/H/B/S/W/D/Q/M1; XSPICE code models (POLY/TABLE), `hertz` B sources and other devices are refused. C goldens `pz_ladder_cur`, `pz_bridge_diff`, `pz_transformer`, `pz_cv_loop`, `pz_diode`, `pz_mos1` and the batch `multi_analysis_pz` (unordered root sets, `compare::POLE_ZERO`), `tests/pole_zero.rs` and opt-in `c_pole_zero`. Deliberate divergences: every finite root is reported where C's search gives up, C's sign-inverted CCVS pole-zero load (`ccvspzld.c`) is not reproduced, dense pencils above 1000 unknowns are refused.
- [x] `.noise` small-signal noise analysis (#100): `analysis::noise` (`noisean.c`/`cktnoise.c`/`nevalsrc.c`/`ninteg.c`) reusing the `.ac` operating point and complex assembly, one LU per frequency with a forward unit-input solve and a transposed adjoint solve (`ComplexLu::solve_transposed`, C `NInzIter`), per-generator `Nintegrate` integration, C's `dec`/`oct`/`lin` loop and single-frequency rules, `pts_per_summary` columns and sampling, and C's two plots (`Noise Spectral Density Curves`, `Integrated Noise` with `v(...)`/`i(...)` names) in batch order (`ScheduledAnalysis::extra_plot_names`, `Analysis::run_plots`). Device hook `Device::noise` (default `NotYetPorted`; explicit `Noiseless` for C's noise-free devices including RF port sources) with generators for R (thermal, KF/AF/EF/LF/WF flicker, `noisy=0`), D (RS, shot, KF/AF), Q (RC/RB(`gx`)/RE, `cc`/`cb` shot, KF/AF), MOS1 (RD/RS, NLEV 0–3 channel and flicker laws, GDSNOI) and S/W, in C's `CKTnoise` instance order. C goldens `noise_rc`/`noise_diode`/`noise_bjt`/`noise_mos1`/`noise_multi` (103 verified, `compare::NOISE`/`NOISE_NONLINEAR`), analytic `tests/noise_analysis.rs` (4kTR, integrated totals, kT/C, flicker, shot) and opt-in `c_noise_reference` (C batch layout, order included). The #132 follow-up adds `set sqrnoise`, `keepopinfo`, SPICE3 MOS1 flicker and `.print noise`; transient noise sources and unsimulated model families remain unsupported. See [NOISE.md](docs/port/NOISE.md).
- [x] `.disto` small-signal distortion (#104): `analysis::disto` (`distoan.c`/`cktdisto.c`/`dkerproc.c`/`dloadfns.c`) on the `.ac` operating point and complex assembly, C's own `dec`/`oct`/`lin` sweep (`lin` measures `pts + 2` frequencies), `distof1`/`distof2` V/I inputs (parser, factory, writer), the 2nd/3rd harmonic plots or, with `f2overf1`, the `f1+f2`/`f1-f2`/`2f1-f2` IM plots with `f2` fixed at `f2overf1 * fstart` as in C, `disto<n>` batch names (`ScheduledAnalysis::extra_plot_names`). Device hook `Device::distortion` (default `NotYetPorted`; explicit `Linear` for R/C/L/K/E/F/G/H/S/W and model passives, `Input` for V/I) with Taylor terms for D (`diodset.c`), Gummel-Poon Q (`bjtdset.c`, via the `Series3` port of `Dderivs`) and MOS1 (`mos1dset.c`), reproducing C's simplified distortion models and four C defects (current-input sign, BJT `vbe + vbc` B-C' kernel without RB, BJT unconjugated `f1-f2` vbb/substrate kernels, MOS1 `2f1-f2` H1-for-H2 kernel). B sources and POLY/TABLE code models are refused. C goldens `disto_diode`/`disto_bjt`/`disto_mos1`/`disto_multi` (114 verified, `compare::DISTORTION`), analytic `tests/distortion_analysis.rs` (diode exponential HD2/HD3/IM closed forms) and opt-in `c_disto_reference`. See [DISTORTION.md](docs/port/DISTORTION.md).
- [x] `.sens` DC/AC sensitivity (#102): `analysis::sens` reproduces C's `sens_sens()` (`cktsens.c`; the reference build has no `WANT_SENSE2` adjoint `.sens2`): C's `sgen` parameter lists, order and names (`inst:kw`, `inst`, `inst_kw`, `v(...)`/`voltage`), `Sens_filter` patterns, `delta = 1e-6 p` forward differences on the operating-point Jacobian (`with_c_jacobian`) or the per-frequency AC matrix, C's `dec`/`oct`/`lin` stepping defects and batch order after `.noise`. Device hook `Device::sensitivity` (default `NotYetPorted`) replays C's records (`devices::sensitivity`: setters, `DEVsetup` defaults and setup-only values, `DEVtemperature`, given flags left by each restoration) for R/C/L (literal and model-backed), K (DC), V/I, E/F/G/H, B/S/W (DC) and D (DC, including C's NaN for an unset knee current). Refused: Q, MOS1, code models, RF ports, AC with nonlinear devices/switches/K (C linearizes them at a reset state), two `.sens` cards or `.sens` with `.sp`. C goldens `sens_divider`/`sens_hot`/`sens_diode`/`sens_ac`/`sens_multi` (119 verified, `compare::SENSITIVITY`/`SENSITIVITY_NONLINEAR`), analytic `tests/sensitivity_analysis.rs` and opt-in `c_sens_reference`. See [SENSITIVITY.md](docs/port/SENSITIVITY.md).

## Suggested sequence

#12/#13 scoped/source syntax → #14–16 expressions/evaluation/options/globals →
#20–22 serialization/snapshots/full M1 round-trip gate (done). #18 flattening landed in M5 wave 1 ([SUBCIRCUITS.md](docs/port/SUBCIRCUITS.md)).
CLI simulation (#6, done) and model elaboration →
SPICE-compatible transient → nonlinear devices → bounded M5 usability (done).
Numerical optimization is follow-up,
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

### crates.io release automation (not started)

The package is ready to publish; the release path is not.

- [ ] Decide the first published version — `0.0.0` is a placeholder, not a release — and whether the crate is ready to claim the `ngspice-rs` name publicly. A published version can be yanked but never deleted or reused.
- [ ] Bootstrap the first release deliberately, outside CI: crates.io only accepts a trusted publisher for a crate that already exists, so the initial publish of the name needs an API token.
- [ ] Add a `release` environment and a `v*`-tag workflow with `permissions: id-token: write` and `rust-lang/crates-io-auth-action`, publishing the single package with `cargo publish`; guard that the tag matches the workspace version and gate the job on the existing `Tests` workflow.
- [ ] Add a `cargo package` check to CI so packaging, the include/exclude set and the package-relative conformance paths cannot regress silently.
- [ ] Clear the six `cargo doc --no-deps` warnings before docs.rs is used: five public docs link to private items (`analysis::fourier` twice, `analysis::measure`, `analysis::newton`, `devices::circuit`) and `netlist::bexpr` links to the unresolved `crate::lower`.


## M9 — Output and front-end completeness (work/m9)

- [x] Binary CLI output (#112): explicit format, front-end/environment precedence,
  atomic publication, process readback and opt-in C loading.
- [x] Bounded ASCII `.plot` rendering (#111): shared operands, deterministic
  line-printer field, atomic failure behavior, opt-in C legend/DC-row comparison.
  Terminal pagination/resampling and explicit limits remain unsupported.
- [x] Subcircuit directives (#108): scoped options/globals/hints/saves, repeated
  measurements, and once-per-used-definition Fourier hoisting; C oracle and RC golden.
- [x] Device observations (#113): requested non-branch terminal currents and
  scalar `@device[param]` asks at solved OP/DC/companion transient points,
  contextual scalar asks in AC, saved R/D/Q/M C golden and opt-in drift check.
  Unsupported asks, coincident terminals, AC current/power asks and diffsol
  observations fail explicitly; see OUTPUT_SELECTION.md.
- [x] Remaining measurements (#114): WHEN, TD, extrema positions, PP, DERIV,
  margins, vector thresholds, Simpson integration, two-pass scalar PARAM/EXPR
  expressions and expression-valued numeric setters, with opt-in C comparisons.
  Upstream ERR variants are themselves unimplemented and explicitly refused.
- [x] Configurable Fourier (#115): settings, polynomial interpolation including
  C's degree fallback/edge behavior, scoped hoisting and fundamental parameter
  expressions (Rust extension); opt-in C comparisons per setting and degree.
  Bounded `set` / `run` / `fourier` / `quit` control blocks are supported;
  the general interactive command interpreter remains outside scope.

All six M9 implementation slices are delivered with the bounded interfaces and
explicit unsupported cases documented in CLI.md, OUTPUT_SELECTION.md,
MEASURE.md and FOURIER.md. This does not imply full SPICE compatibility.

## M10 — Extended device library (docs/port/M10.md)

- [x] URC uniform distributed RC lines (#85, part 1; `work/m10-urc`): `inp2u.c`
  grammar (`l=`/`n=`, required model, refused leading value), `urc` model
  schema with `urcsetup.c` defaults and the no-op `urc` flag, and `urcsetup.c`'s
  expansion (FMAX section rule, geometric `K` scaling, `ISPERL` diode ladder
  with the generated `<name>#diodemod`) into existing R/C/D devices with C's
  names (`u1#hi1`, `u1#rlo1`, …) plus the load-free instance answering
  `@u1[l]`/`@u1[n]`. C goldens `m10_urc_tran`/`m10_urc_ac`/`m10_urc_diode_tran`
  (123 verified), `tests/urc_lines.rs` and opt-in `c_urc_reference` (element
  asks equal to C's to 1e-15). Explicit errors for C's degenerate inputs
  (missing `l`, `n < 1`, `K = 1`, zero `CPERL`), `.pz` (C aborts) and `.sens`
  (`NotYetPorted`: C's zero URC parameter entries). See
  [URC.md](docs/port/URC.md).
