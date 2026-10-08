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
| Parser | `.param`/`{expr}` syntax (#14) with top-level evaluation (#15), scalar R/C/L/V/I, declared-model passives, models, bounded D/Q/M flags/IC vectors, PULSE/PWL (elaborated to forcing, #9), scoped subcircuits/X and resolved includes/libraries; 8/8 fixture parses and normalized round trips (M1 gate, #22); not simulation |
| Model inputs | Top-level first-wins resolver, family/level checks, bounded passive factories and diode input schemas |
| Passive models | R sheet/C area-perimeter geometry, scalar model L, TC1/TC2/TEMP/TNOM, scale and multiplicity |
| Topology | Petgraph circuit incidence and matrix-row graphs, simulation branch-row binding |
| Linear equations | faer real/complex LU, scalar R/C/L/V/I elaboration and stamps; opt-in library equilibration wrappers, original-unit residuals and unchanged defaults (#46, [EQUILIBRATION.md](docs/port/EQUILIBRATION.md)) |
| Numerical follow-up | #47 backend/rank audit retains production guards (no enabled optimization or formal certificate; proof follow-up #68); #29 constrained-RLC prototype/ADR is not production-enabled; runtime gates #69–#72 |
| DC / AC | Linear and bounded nonlinear `.op`, typed/nested source/temperature `.dc`, bias-linearized `.ac` |
| Transient | Ordinary `.tran`: adaptive trapezoidal / Gear-2 companion driver with truncation-error control and breakpoint landing (#26, [TRANSIENT.md](docs/port/TRANSIENT.md)), linear and bounded nonlinear charge paths, linear `.ic`/`uic` supported and nonlinear initialization rejected; explicit `backend=diffsol method=bdf` adaptive BDF for index-one DAEs, including floating/coupled capacitor mass blocks; higher-index pencils rejected |
| Nonlinear devices | Bounded diode/Ebers-Moll BJT/MOS1 DC/AC/charge-companion paths; [M4 support/gate](docs/port/M4_NONLINEAR.md) |
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
- [x] Use directed petgraph dependency graphs for subcircuit elaboration (#18, M5): `expand_subcircuits` builds the reachable definition graph and rejects recursion via its SCCs. See [SUBCIRCUITS.md](docs/port/SUBCIRCUITS.md).
- [x] Enable `subckt_divider` AST/CLI tests: all eight fixtures parse (the M1 round-trip gate is #22). With #18 the deck also simulates through the production `.op` path and is verified against its C golden. See [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md).

### M1d: parameter semantics and full front-end gate

- [x] Parse `.param` cards and a bounded numparam expression grammar (#14): winnow precedence, C-pinned `^`/sign rules, function allowlist, braced values at device/model/analysis/X sites, positioned unevaluated AST. See [PARAM_EXPRESSIONS.md](docs/port/PARAM_EXPRESSIONS.md); no evaluation.
- [x] Evaluate top-level `.param` values (#15): C-backed order/redefinition/forward-reference rules, petgraph cycle and undefined-name chains, bounded work, finite-or-error arithmetic (`spice_netlist::eval`), and a literalized netlist copy consumed by `Circuit` elaboration and `RunConfig::request_for` (`spice_netlist::elaborate`). Opt-in C probes: `c_param_eval`. See [PARAM_EXPRESSIONS.md](docs/port/PARAM_EXPRESSIONS.md).
- [x] Subcircuit formal defaults/overrides and scoped evaluation (#18, M5): `X` instances expand through the production entry points with hierarchical names, per-instance parameter/model scope and `.global`; precedence is instance override > body `.param` > formal default. See [SUBCIRCUITS.md](docs/port/SUBCIRCUITS.md).
- [x] Quoted `'expr'` values and `.func` definitions (#107): single quotes delimit an expression wherever braces do (C `inp_change_quotes()`); `.func` cards (and the `.param f(x)=body` spelling C rewrites to `.func`) at the root and in `.subckt` bodies with lexical scoping, last-definition-wins, built-in override, by-value calls with call-site free names, petgraph recursion detection and arity checks (a `.subckt` body's only when instantiated, as in C; `v()`/`i()` device-value calls are `NotYetPorted`) (`spice_netlist::eval::FunctionScope`, `ParamScope::for_netlist`). Opt-in C probes: `c_func_eval`; golden `func_quotes`. See [PARAM_EXPRESSIONS.md](docs/port/PARAM_EXPRESSIONS.md).
- [x] `{expr}` and `'expr'` option values (#107, option part): parsed into `OptionSetting::expression`, evaluated by `RunConfig::from_netlist` against top-level `.param`, writer/semantic round trip, opt-in C probe (`c_options_reference`). See [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md).
- [x] Broaden `.option` coverage (#110): junction `gmin`, `itl2` (continuation stage limit of every DC bias, `.dc` warm starts), `itl4`, `itl6`, `xmu`, DC options for the companion `.tran` initial bias, documented no-ops (print flags, C-ignored `itl3`/`itl5`/..., `post`/`ingold`, `bypass=0`) with `RunConfig::ignored`; `itl1`/`itl2`/`itl4` follow C's effective `max(n, 100)` (`niiter.c`), BJT gmin scales with `m`; the rest (incl. `pivtol`/`pivrel`) stay `NotYetPorted`. Opt-in C comparisons in `c_options_reference`, goldens `options_gmin_dc`/`options_xmu_tran`. Status table in [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md#option-coverage-110). Still pending: `pivtol`/`pivrel` (no LU pivot-threshold knob), `gshunt`/`cshunt`/`rshunt`, `noopiter`, `minbreak`, `numdgt`, front-end output variables; `.option gmin` does not yet seed the DC gmin-stepping ladder, and C's adaptive `dynamic_gmin`/`gillespie_src` step control (raw `itl2/4`) has no counterpart in the fixed ladders.
- [x] Parse `.option` and `.global` (#16): ordered positioned settings, `spice_analysis::RunConfig` for temp/tnom/reltol/vntol/abstol (method/maxord retained, rejected for `.tran`; all other options error). Top-level `.global` contract consumed by the #18 flattener; `.option`/`.global`/`.ic`/`.nodeset` inside a body remain rejected. See [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md).
- [x] Add normalized-deck serialization preserving source parameter application order (#20): `spice_netlist::write_netlist` plus location-free `semantic_eq`/`semantic_diff` (contract in [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md)); directives are written, resolved include content is not inlined. The eight-fixture gate is closed by #22 below.
- [x] Commit deterministic token/AST snapshots and document regeneration (#21): `spice_netlist::dump`, 66 files under `conformance/snapshots/`, `cargo xtask snapshots [--bless]` (see `conformance/snapshots/README.md`).
- [x] Close the M1 front-end fixture and round-trip gate (#22): `crates/spice-netlist/tests/m1_gate.rs` pins per-deck device/terminal/model/analysis expectations for all eight `conformance/netlists` decks, parse→write→parse semantic equality with a writer fixed point, matching committed token/AST snapshots, combined fixtures `conformance/parser/combined_{includes,params_options}.cir` (includes + subcircuit + params + options; include provenance and setter order), and negative coverage (malformed cards, unsupported forms, include/parameter cycles, scoped-name containment). `crates/spice-analysis/tests/m1_combined.rs` runs the params+options fixture through `RunConfig`. Scoped-name enforcement is structural only: declarations stay in their scope and Q-family lookup sees only the local scope and ancestors; unresolved X targets and D/M model names are *not* resolved at parse time (elaboration owns that).
- [x] `cargo xtask ci` is green with the gate; the M1 front-end gate is closed. **Not part of this claim and still outstanding:** `{expr}` option values, quoted/`'expr'` values, `.func`, directives inside a body (`.option`/`.global`/`.ic`/`.nodeset`), scoped model resolution/binning (quoted values and `.func` later landed with #107). D/Q/M decks parse only; this is not nonlinear simulation parity. (#18 later added subcircuit elaboration on top of the gate; see [SUBCIRCUITS.md](docs/port/SUBCIRCUITS.md).) C parser oracles remain opt-in (`NGSPICE_BIN`).

## 3. Model elaboration and validation

- [x] Top-level first-declaration model lookup, family compatibility and family-specific level selection/rounding (#17); failed elaboration leaves circuit state unchanged.
- [x] Device-owned scalar schema extension API and bounded diode IS/N/RS/AREA/TEMP/TNOM defaults/ranges (#17); raw AST preserved; M4 adds bounded model-aware D/Q/M factories (see its support table).
- [ ] Add scoped model resolution, binning and further device-owned schemas/defaults; expand only with production tests.
- [x] Preserve omitted versus explicit BJT substrate terminals; bounded M4 factories reject unavailable substrate physics/backends.
- [x] Define and implement the bounded passive value/geometry/temperature surface (#19); explicit errors for unsupported setters, missing/invalid geometry and nonfinite derivations.
- [ ] Expand passive aliases, coil geometry, DTEMP/TCE/AC-only values and other advanced forms only with documented formulas and conformance tests.

## 4. Complete SPICE-compatible transient analysis — M3

Complex linear AC is already implemented on main. Bounded adaptive
BDF does not close the following trap/Gear and general-transient requirements.

- [x] Implement trapezoidal and Gear orders 1–2 coefficients, companion integration, prediction and per-element truncation estimates (`src/maths/ni/`, `cktterr.c`) with trial coefficients separate from accepted step history (#23); Gear orders 3–6 are rejected.
- [x] Implement adaptive timestep scheduling and truncation-error control in a companion transient driver (#26): `spice_analysis::companion_transient` / ordinary `.tran`, trap and Gear-2 (`maxord` 1-2), `dctran.c` step/order/breakpoint policy, accepted points as output, work/min-step limits; analytic RC/RL/RLC and PULSE tests, `rc_transient` registered in `golden verify`, opt-in live C agreement ([TRANSIENT.md](docs/port/TRANSIENT.md)). Linear circuits only; Newton structure is in place for M4.
- [x] Implement C/L trap/Gear-2 companion stamps from accepted charge/flux state without double-discretizing diffsol equation stamps (#25); independent sources stamp their time-`t` forcing (left/right limit) in companion loads (#26); mutual inductance is pending (`ic=`/`uic` landed with #27).
- [x] Implement `.ic`, `.nodeset`, instance IC and `uic` semantics with consistent constraints (#27): `.ic`/`.nodeset`/`uic` flow from `RunConfig` into every `AnalysisRequest`; the companion `.tran` enforces `.ic` as exact row constraints in the initial bias (ideal-source conflicts are errors), starts `uic` runs from capacitor/inductor `ic=` charges and fluxes with an exact impulse-freedom check, treats `.nodeset` as a validated no-op for linear circuits (C quirk: used as node IC under `uic`), validates nodes in every analysis, and keeps `backend=diffsol` explicit-reject; analytic RC/RL/RLC tests and opt-in live C agreement ([TRANSIENT.md](docs/port/TRANSIENT.md)). Mutual inductors and nonlinear device initial conditions remain pending.
- [x] Connect parsed PULSE/PWL source waveforms to time evaluation and breakpoint handling (#9): validated analytic `Pulse` with C defaults resolved from the `.tran` step/stop (`PulseSpec`/`TransientTiming`), `Waveform::value_at(t, Limit)` and lazy `breakpoints_in(t0, t1)`; diffsol BDF stops at every corner/jump (budget 100k segments) and starts from the t=0 forcing; the companion driver (#26) lands on every breakpoint lazily. PULSE pulse count, PWL `r=`/`td=` and SIN/EXP/SFFM/AM followed in M6 (#94, #95, next item).
- [x] M6 source functions (#94, #95): positioned `SIN`/`SINE`/`EXP`/`SFFM`/`AM` syntax (`SourceWaveform::Function`), the PULSE eighth field (pulse count in ngspice's default compatibility mode) and PWL `td=`/`r=` as ordered scalar setters, for V and I sources; writer round trips. `spice_devices::functions` resolves C's per-field defaults from `.tran` (`FunctionSpec::resolve`), evaluates analytic values with left/right limits, enumerates lazy corners (the port also lands on SIN/SFFM/AM delays and EXP `TD1`/`TD2`, where C sets no breakpoint) and gives OP/DC the C time-zero value (SFFM/AM: 0). Five C goldens (`rc_sin_tran`, `rc_exp_tran`, `rc_sffm_am_tran`, `rc_pwl_repeat_tran`, `rc_pulse_count_tran`) verify under `compare::TRAN`; repeated-PWL boundaries are left/right jumps in both backends (C's single boundary value depends on ulp-level landing; the measured divergence, and that of the port's extra SIN/EXP/SFFM/AM/fractional-PULSE breakpoints, is bounded by an opt-in unmarked-deck comparison); a `.four` analytic spectrum/THD test on a SIN-driven RC; opt-in `c_source_functions` compares 22 sources' values with C at C's own timepoints, `.op` time-zero values and a SIN-driven clipper's THD. See [TRANSIENT.md](docs/port/TRANSIENT.md#source-functions-94-95). The diffsol BDF backend rejects SIN/EXP/SFFM/AM explicitly (it interpolates forcing linearly between breakpoints). Still unported: TRNOISE/TRRANDOM/EXTERNAL, PWL `file=`, expression-valued waveform fields, negative delays, ngspice's `xs` compatibility (PULSE phase).
- [x] Demonstrate an index-one formulation for floating/coupled capacitor networks in the BDF backend: block-SVD `ker E`/`ker Eᵀ`, rank-checked `Wᵀ A N`, charge-preserving event projection and consistent derivatives, with analytic and opt-in C tests (#28).
- [x] Deliver the bounded higher-index formulation/prototype gate (#29): fallible fully voltage-constrained RLC reduction/reconstruction, original/differentiated residuals, IC/rank/impulse refusal and analytic tests; production pencils remain rejected. See [HIGHER_INDEX_DAE_ADR.md](docs/port/HIGHER_INDEX_DAE_ADR.md); runtime enabling is separately gated by #69–#72.
- [x] Define explicit trial-versus-accepted device state: `&self` trial loads into a disposable `TrialState`, rotating `StateHistory`, per-device branch/state ranges and integration context, atomic accept hooks before commit (#24).
- [x] Accepted-state/trial-state separation, event left/right limits, distinct voltage/current tolerances and progress/work budgets in the companion driver (#26); tests show rejected trials never advance histories and accept-hook failures abort the run. Every future Newton/nonlinear driver must keep this.
- [x] Complete RC/RLC transient and AC C-golden exit gates on common physical sample grids; do not require identical adaptive timesteps (#48). `xtask/src/tran.rs` event-aware comparator with `compare::TRAN`; twelve fixtures registered in `golden verify` at that point, sixteen with the initialized-state fixtures below (8 new: RL/RC-Gear/RC-PWL/RLC trap+Gear/floating/coupled `.tran`, RLC `.ac`), with Rust-only explicit-BDF variants against the same goldens (`TRAN`, or the peak-scaled `TRAN_RESTART` where C's backward-Euler restart error exceeds the pointwise bound); analytic closed-form, KCL, charge, energy and production-API gate tests in `crates/spice-analysis/tests/m3_gate.rs` ([VERIFICATION.md](docs/port/VERIFICATION.md)).
- [x] Initialized-state (`.ic`/`uic`/instance `ic=`) conformance fixtures (#48 remainder): `rc_ic_uic_tran` (RC discharge from `ic=2`), `rlc_ic_uic_tran` (series RLC from inductor `ic=20m` and capacitor `ic=1`), `rc_ic_node_tran` (`.ic v(out)=0.25` without `uic`: constrained initial bias, then released) and `floating_cap_ic_tran` (floating capacitor with a 2 uC plate charge). New C goldens captured one at a time with `cargo xtask golden capture --netlist <name>` (no existing golden touched), registered in `golden verify` with `compare::TRAN` unchanged (16 verified, worst error 0.000 of the bound); no BDF variants because the diffsol backend rejects `.ic`/`uic`/`ic=` (a test asserts the rejection). The comparator's `tran::Grid` gained a `start` for `uic` runs (C writes no `t = 0` row; both plots must begin at the same first accepted step). Closed-form decay, conserved plate charge, `uic` first-row/step-breakpoint and `.ic`-release checks in `crates/spice-analysis/tests/m3_gate.rs`.
- Still blocked / unsupported by the gate (do not claim M3 or universal MNA DAE support): runtime higher-index source constraints (#69–#72), nonlinear initialization and physics beyond the M4 support table, orders above 2.

## 5. Nonlinear devices and convergence — M4

Bounded M4 baseline merged in PR #58. Local `work/m4-followups` adds #34/#35
acceptance work; it has not been published and those issues remain open.
Exact schemas, physics exclusions and #41 evidence: [M4_NONLINEAR.md](docs/port/M4_NONLINEAR.md).

- [x] Reuse typed first-declaration model resolution and device-owned ordered schemas/defaults; extend bounded diode/BJT/MOS1 factories atomically.
- [x] Implement diode and Ebers-Moll BJT junction/charge equations and bounded MOS1 square-law/body/overlap charge with analytic Jacobian stamps.
- [x] Add reusable Newton iteration with global voltage-step damping, physical residual checks, source/nodal-gmin stepping and request/deck physical tolerances. Full C PN/FET limiting/control-option parity is not claimed.
- [x] Add typed independent-source/temperature/scalar-resistor DC targets and one nested outer axis; preserve originals, validate reachable grids, rebuild R/TEMP operators, retain source-only linear LU reuse (#35 local follow-up; [DC_SWEEPS.md](docs/port/DC_SWEEPS.md)).
- [x] Add bounded configurable DC continuation schedules/budgets, success/failure stage reports, and request > deck > default controls shared by OP/DC/AC bias (#34 local follow-up; [DC_CONTINUATION.md](docs/port/DC_CONTINUATION.md)); explicit continuation controls reject for transient.
- [x] Implement bias-linearized AC and actual nonlinear Q-based trap/Gear-2 companions, multi-charge LTE and disposable trial/atomic accepted history.
- [x] Demonstrate diode DC, BJT bias and MOS1 operating point against existing C data; add six C AC/charge-transient decks, physical/Jacobian/conservation/continuation checks and explicit tolerances for local #41 subset.
- [x] Reject unimplemented parsed physics; BSIM/CIDER/XSPICE and full SPICE parity remain outside scope.
- [ ] Expand beyond this demonstrated subset only with new production conformance: non-nominal junction charge/BJT/MOS temperatures, BJT Early/high-injection/substrate/series physics, MOS intrinsic channel charge (nonzero TOX), nonlinear .ic/uic, arbitrary model-setter sweeps and full C dynamic convergence/limiting parity.

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
- [x] Behavioural sources (#79): B `v=`/`i=` expressions (C's `inpptree` grammar over the raw card text; braces/quotes transparent; 11-digit literal rounding; `.param`/`.func`/`=pwl(` numparam values resolved per scope) with the full `inpptree.c` function set, compiled to a value-and-gradient evaluator applying C's derivative rule per node (quirks included); `ASRCload` Newton stamps, `ASRCacLoad` AC Jacobian, `time`/`temper`/`hertz` (AC re-solves the bias per frequency, `CKTvarHertz`), `m`/`tc1`/`tc2`/`temp`/`dtemp`/reciprocal setters, `i()` of V/E/H/voltage-B branches; no breakpoints (as C); Newton's voltage-step damping skips behavioural outputs. E/G `VALUE`/`VOL`/`CUR`, `TABLE` (XSPICE `pwl` map), `POLY(n)` on E/G/F/H and the implicit `POLY(1)` are lowered like `inpcom.c`/`enhtrans.c` before subcircuit expansion, with `inp_meas_current()` current sensing. C goldens `bsource_op`/`bsource_dc`/`bsource_ac`/`bsource_tran`/`evalue_op`/`gtable_dc`/`epoly_dc` plus the zero-start `bsource_zero_op`/`bsource_zero_dc`/`bsource_zero_tran` in `golden verify` (48 verified; XSPICE code models loaded via `* xtask-codemodels:`), FD-Jacobian tests for every function and an opt-in 1e-12 C value/derivative oracle (`c_behavioural_reference`). Newton retries a numerically failed linear solve with Curtis-Reid balancing and iterative refinement, so the `1e32` zero-start slopes of `1/x`/`sqrt`/`log` converge as in C. Still unported: `ddt`, the statistical functions `agauss`/`gauss`/`aunif`/`unif`/`limit` (`NotYetPorted`), infinite slopes at a Newton iterate (`v(x)^0.5` from 0 V stops where C continues through a NaN iterate), `LAPLACE`, compatibility-mode variants, user XSPICE cards, the BDF backend. See [BEHAVIOURAL_SOURCES.md](docs/port/BEHAVIOURAL_SOURCES.md).

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
