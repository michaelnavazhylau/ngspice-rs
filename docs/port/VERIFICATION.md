# Verification

## Bounded numerical follow-up gate (#46, #47, #29)

Parent validation of the integrated candidate tree reports **824 passed, 0 failed,
37 ignored** on stable (rustc 1.99.0) and Rust 1.89.0; combined maths **72/0/0**.
Workspace/all-target Clippy `--locked -- -D warnings` passes on both toolchains;
formatting and whitespace checks are clean. `cargo xtask golden verify` remains
**26 verified / 0 unsupported / 0 failures**; all **37 ignored live-C** checks
pass with absolute `NGSPICE_BIN`, and `golden check` reproduces all **26** C
fixtures. Existing goldens, 110 parser snapshots, dependencies and `Cargo.lock`
are unchanged.

- #46: **8** production-interface analytic scaling tests; independent physical
  values/residual bounds, snapshots, symbolic-pattern/rank/range errors and
  measured acceptance/overhead probes ([EQUILIBRATION.md](EQUILIBRATION.md)).
- #47: **6** guard tests, backend audit and example-only benchmark comparing
  actual production classification at widths 1/8/16. Parent reruns retain all
  **42 cases per width (24 accept / 18 reject)** and failed probes. Timing/RSS
  limitations and the aggregate-contraction proof caveat are explicit
  ([SPARSE_RANK_DIAGNOSTICS.md](SPARSE_RANK_DIAGNOSTICS.md)); formal certificate
  gate #68 remains unresolved. Numeric policy/thresholds are unchanged.
- #29: **11** analytic/error/residual/prototype tests plus unchanged index-one
  regressions. This is a numeric-only smooth constrained-RLC formulation gate,
  **not production enablement** ([HIGHER_INDEX_DAE_ADR.md](HIGHER_INDEX_DAE_ADR.md));
  separate waveform/topology/integration/event gates are #69–#72.

A fresh fourth read-only agent inspected exact candidates and actual logs:
no candidate-caused P0/P1/P2; all three **OK with notes**. Parent corrected
certification wording and carried inherited/prototype limits into capability
summaries before revalidation. Finite tests are not a universal uniqueness proof,
and normwise backward checks do not promise componentwise/forward accuracy.
The slice counts below are historical delivery evidence, not current totals.

## Historical bounded M5 gate (#18, #6, #45, #42, #43, #44)

`cargo xtask golden verify` reports **26 verified fixture(s), 0 unsupported
fixture(s), 0 failure(s)**: `EXCLUDED` in `xtask/src/verify.rs` is empty, so
`subckt_divider` runs through the production `.op` path and matches its committed
C golden. `cargo test --workspace --locked` reports **799 passed, 0 failed, 37
ignored** on stable (rustc 1.99.0) and Rust 1.89.0; the 37 opt-in live-C
comparisons pass with `NGSPICE_BIN` set. Subcircuit instantiation:
[SUBCIRCUITS.md](SUBCIRCUITS.md); CLI `simulate` (one analysis, ASCII rawfile):
[CLI.md](CLI.md); binary rawfile read/write: [RAWFILES.md](RAWFILES.md);
`.save`/`.print` output selection: [OUTPUT_SELECTION.md](OUTPUT_SELECTION.md);
`.measure`/`.meas` measurements: [MEASURE.md](MEASURE.md); final-period `.four`:
[FOURIER.md](FOURIER.md). All six bounded M5 deliverables are complete
([ROADMAP.md](ROADMAP.md)); `.plot` and the documented extended forms remain
unported. Fourier C checks compare all nine harmonic magnitudes and at least
five significant phases per vector; a temporary +90° phase mutation failed all
three tests, then passed after restoring production code. Clippy on both
toolchains, formatting and whitespace checks pass; goldens, 110 parser snapshots
and `Cargo.lock` are unchanged by the Fourier slice.
The historical per-slice counts below record the state at each
delivery; they are not the current totals.

## Branch-local M4 gate (#41)

The nonlinear support/gate is documented in [M4_NONLINEAR.md](M4_NONLINEAR.md).
`cargo xtask golden verify` verifies 26 fixtures (nine nonlinear), including
`subckt_divider` through the production `.op` path, with no exclusions. Six new C
AC/charge-transient goldens and twelve
parser snapshots were added without changing previous data. Physical/Jacobian/
charge/continuation/ownership checks live in `spice-analysis/tests/m4_gate.rs`.
The historical linear sections below describe their original M2/M3 delivery;
they do not supersede M4's explicit supported-physics and tolerance table.

## The principle

The C tree is never linked. The port is compared against the reference
implementation **out of process**: `cargo xtask golden capture` drives the C
`ngspice` binary over a fixed set of decks and stores what it prints, and the
test suite reads those stored files. The workspace lint `unsafe_code = "forbid"`
exists to keep it that way — there is no FFI surface to widen by accident.

Two consequences worth stating plainly:

- `cargo test` needs neither the C tree nor a built `ngspice`. The goldens are
  committed data, so the suite runs anywhere.
- Drift is detected by re-running the reference implementation:
  `cargo xtask golden check`. Nothing in the port's own tests can detect that the
  C binary changed under it.

## Fixtures

`conformance/netlists/*.cir` are **pure decks**: no `.control` section, exactly
one analysis card, no file I/O. `conformance/netlists/README.md` explains why,
and the table there says what each fixture exercises.

## Capturing

`ngspice -b -r out.raw deck.cir` does not produce a usable rawfile here: with no
`set filetype` the output is binary, and `-r` is honoured before any control
script can change that. So `xtask` instruments each deck by inserting

```spice
.control
set filetype=ascii
run
write <fixture>.raw
.endc
```

immediately before the first `.end` card, runs `ngspice -b` on it with a scratch
directory (`target/xtask/golden/<fixture>/`) as the working directory, and reads
the ASCII rawfile that `write` produced. A fixture that already contains a
`.control` section is rejected rather than instrumented.

The scratch directory is also why `write` takes a bare file name: the path never
needs quoting, and `.include`-style relative paths would resolve the same way
every time (they are out of scope for fixtures anyway).

## Storage and comparison

Goldens are stored **verbatim**: `conformance/golden/<fixture>.raw` is the byte
sequence `write` produced, minus nothing. Faithfulness matters more than tidy
diffs, and it means a golden can always be traced back to a command.

Comparison ignores exactly one line, the `Date:` header, because it comes from
the clock. Everything else is compared, including `Command:`, which carries the
`ngspice-<version>` string — so a change of reference binary is reported as
drift rather than silently absorbed.

## Commands

| Command | Effect |
| --- | --- |
| `cargo xtask golden capture` | capture every fixture, write `conformance/golden/*.raw`, report new/changed/unchanged |
| `cargo xtask golden check` | capture into the scratch directory and report drift; writes nothing, exits non-zero on drift |
| `cargo xtask golden list` | describe each committed golden: plot name, variable count, point count, finiteness |
| `cargo xtask golden verify [--netlist <NAME>]` | run the supported Rust analyses against committed C data; no C binary, no writes, non-zero on failure |
| `cargo xtask snapshots [--bless]` | check/regenerate token and AST snapshots; Rust only; non-zero on drift without `--bless` |
| `cargo xtask ci` | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` |

For **capture/check only**, locate the reference binary with `--ngspice <PATH>` or `NGSPICE_BIN`; otherwise
`build/src/ngspice` under the workspace root is tried (a convenience that only
exists inside a built ngspice checkout), then `ngspice` on `PATH`.
A relative `--ngspice` is resolved against the workspace root, not the caller's
directory, and stored as an absolute path so it still resolves inside the scratch
directory.

## What the tests check

`crates/spice-analysis/tests/golden_rawfiles.rs`:

| Test | Claim |
| --- | --- |
| `every_fixture_has_a_golden_and_no_golden_is_orphaned` | the two directories are in one-to-one correspondence |
| `every fixture is documented in EXPECTATIONS` | a new golden cannot be added without a human writing down what it contains |
| `fixtures_have_the_expected_shape_and_values` | plot name, flags, point count, variable names and hand-checked values |
| `goldens_are_well_formed_and_finite` | every plot has headers, variables with units, at least one point, and only finite values |
| `ngspice_lowercases_the_deck_title` | see the findings below |
| `the_writer_agrees_with_ngspice_apart_from_non_round_trippable_decimals` | the rawfile writer reproduces ngspice's layout line for line |
| `exactly_one_golden_token_is_not_round_trippable` | the one tolerated difference is bounded and named |
| `the_writer_reaches_a_fixed_point` | a second pass through the writer changes nothing |
| `the_transient_golden_matches_the_analytic_rc_curve` | the transient golden is physics, not just recorded numbers |
| `the_diode_golden_is_self_consistent` | sweep monotonicity, bounds, and Kirchhoff on the resistor |

The analytic and self-consistency tests matter as much as the value checks: a
golden captured from the wrong deck, or from a broken build, is internally
consistent but wrong. Checking it against a closed-form solution is what makes
the data trustworthy.

## Findings

Behaviours discovered while capturing, which the port has to reproduce. Each is
pinned by a test so that it cannot be lost.

1. **ngspice lowercases the deck title.** `Title: rc divider, operating point`
   for a deck whose first line is `RC divider, operating point`.
2. **The point index is not tab-separated.** `raw_write()` prints `" %d"`
   followed by `"\t%.*e\n"` for each value, and a blank line after every point.
   So the first value line of a point is `" 0\t<value>"` and the rest are
   `"\t<value>"`, with `""` between points.
3. **A complex plot has two spellings for a zero imaginary part.** A vector
   ngspice has flagged real is written `re,0.0`; one it has not is written
   `re,0.000000000000000e+00`. `.ac` flags nothing real, so even `frequency` and
   `v(in)` carry the long form — this is why `spice_analysis::Variable` has an
   `is_real` flag rather than inferring it from the data.
4. **`%.15e` is not always a round-trip.** A rawfile value carries 16
   significant digits, and two adjacent doubles can share one 16-digit spelling.
   Exactly one token in the current corpus shows this: `time` on line 19 of
   `rc_transient.raw` is written `1.000000000000000e-11`, but the nearest double
   to that decimal prints as `9.999999999999999e-12`. Re-serialising a golden is
   therefore not always byte-exact, which is why the writer is checked by
   numeric comparison plus a fixed-point property. This does **not** weaken the
   port's parity goal: when the Rust engine computes the same double ngspice
   computed, the same `%.15e` formatting produces the same bytes.
5. **`TEMP` and `TNOM` default to 27 °C.** `Doing analysis at TEMP = 27.000000
   and TNOM = 27.000000` on every run, matching
   `AnalysisContext::default()`.
6. **A DC sweep writes a blank line between points**, which the rawfile parser
   has to skip; a blank line is therefore not a reliable plot separator inside
   the values section.

## M1a parser verification

`crates/spice-netlist/tests/linear_parser.rs` checks AST fields against the
committed divider, AC low-pass and RLC decks: terminal order, values, source
locations, request arguments, case folding and ground aliasing. It also checks
the unflattened `subckt_divider` AST (#12). `rc_transient` has a positioned
waveform AST test; the three nonlinear fixtures have M1b tests. All eight
fixtures parse without implying simulation or the M1 round-trip gate. `crates/spice-cli/tests/parse.rs` checks
process exits: supported parse = 0, missing file = 2, unported syntax = 3.

A separate opt-in oracle uses `conformance/parser/linear_sources.cir` to compare
parsed scalar parameters with C's `print @instance[parameter]` after an `.op`
setup. It runs out of process in a unique temporary directory and uses no FFI:

```sh
NGSPICE_BIN=/path/to/ngspice cargo test -p spice-netlist --test c_reference -- --ignored
```

This pins bare-AC defaults (magnitude 1, phase 0), implicit DC zero, source
leading-DC precedence, and R/C/L values and initial conditions. The last-set
parameter map from Rust is compared numerically with C instance queries. The
probe uses simple values; its tolerance is `1e-12` relative, with a `1e-12` scale
floor for zero. The shared oracle sets `numdgt=17` for scalar-query precision.
The tests are ignored by default so ordinary tests remain C-toolchain-independent.
It was run successfully against the local ngspice-47+ binary for M1a.

`conformance/parser/` is **not** part of the rawfile fixture corpus; do not add
`.raw` files there or confuse these instance-query checks with engine parity.
Token/AST snapshots (#21) are committed under `conformance/snapshots/` and
checked byte for byte by `crates/spice-netlist/tests/snapshots.rs`;
`cargo xtask snapshots` reports drift and `--bless` regenerates (Rust only, no C,
fixed point, never touches goldens). Schema, layout, path/Windows rules and the
schema-change procedure: `conformance/snapshots/README.md`. The eight-fixture
round-trip gate is `crates/spice-netlist/tests/m1_gate.rs` (#22); ordinary tests
never need C, and C parser oracles remain opt-in.

`crates/spice-netlist/tests/c_param_reference.rs` is a further ignored oracle for
the `.param`/expression grammar (#14): it folds parsed trees with a test-local
evaluator and compares the values with C's numparam, pinning precedence,
associativity and the leading-sign rules. Run it with
`NGSPICE_BIN=/abs/path/ngspice cargo test -p spice-netlist --test c_param_reference -- --ignored`.
The fixture `conformance/parser/param_expressions.cir` is parsed by ordinary
tests and the CLI without claiming any value is resolved.

## Winnow backend regressions

The existing M1a contracts are retained; fixture/CLI expectations now also
include the M1b diode/BJT/MOS decks, and the live oracle shares its runner across probes.
`crates/spice-netlist/tests/winnow_parser.rs` adds checks that cuts preserve
terminal and missing-value diagnostics, optional slots cannot swallow overflow,
repetition cannot hide unported expressions, AC lookahead leaves following
keywords untouched, and trailing device tokens are never silently ignored.
Unicode-node diagnostics remain byte-column-based and separate parser calls
cannot leak backtracking state.

Cache the locked dependencies with `cargo fetch --locked` before an offline
check (`cargo test --workspace --locked --offline`). The zero-external-dependency
claim applies to the historical M0/M1a backend, not the `new-parsing` rewrite.

Worktree-only publication checks run via
`bash scripts/tests/publish-rust-only.sh`. They use disposable local repos and a
local bare remote to verify branch routing and dirty/mismatched-target refusal;
they never contact GitHub and are not part of `cargo xtask ci`.

## M1b model/diode slice verification

`crates/spice-netlist/tests/model_diode_parser.rs` covers the `diode_dc` AST,
model families/forms, raw first level, ordered duplicates, numeric model names,
forward/unresolved references, scalar geometry, ground aliasing, continuation
provenance, committed malformed/overflow errors and specific unsupported gaps.
At that slice's completion, CLI tests required six fixture parses; #8 now adds
`rc_transient`, bringing the count to seven at that stage. #12 adds
`subckt_divider`: all eight exited 0 for parsing at that stage; #18 later added
subcircuit elaboration, so the deck now simulates through the production `.op`
path.

The second ignored test in `c_reference.rs` instruments
`conformance/parser/model_diodes.cir`. Live C instance/model queries check
leading-area precedence, `perim`→`pj`, scalar geometry/IC/temperatures, implicit
area, duplicate model setter order and basic scalar model parameters. Both
probes pass against ngspice-47+. Neither probe proves selector rounding,
model resolution, backend availability, arbitrary keyword validity, derived
geometry/defaults or Rust simulation arithmetic. Those boundaries remain
explicit in the AST and architecture docs.

## M1b BJT/MOS slice verification

`crates/spice-netlist/tests/transistor_parser.rs` adds 18 regressions for Q/M
fixture ASTs, optional substrate vs earliest declared model, alpha model names
with digits and numeric node names, forward declarations, raw ordered scalars,
leading BJT area, MOS bulk/model collisions, omitted ports, overflow, unsupported
advanced flags/extra ports/binning, and byte-column provenance. #10 adds
separate valid/malformed IC vector and bare-flag regressions below. Declaration
indexing stops at `.end`, excludes unsupported scope bodies, preserves error
order, and has no state shared across parser calls.

The third ignored oracle instruments `conformance/parser/transistor_scalars.cir`.
It compares scalar setters and external terminal IDs after C setup with the
parsed port order (including C's grounded omitted Q substrate), and covers
keyword-like model names and ordinary model names containing digits. The fourth
ignored test pins ngspice-47+'s rejection of Q model names `123` and `123n`:
Rust emits Parse for the former and an explicit numeric-model gap for the latter.
All four live tests pass; these remain syntax/setup checks, not Rust simulation.
Selector defaults/rounding, family compatibility, scoped model resolution,
node/model `gnd`/`0` collisions, model defaults and advanced device arithmetic
are not proven by these probes.

## Passive syntax and model-schema verification

PR #51 merged #11/#17. This section records their initial validation; the
bounded passive elaboration section below records #19's additional checks.
`passive_models.rs` adds 11 syntax regressions, and `spice-devices/tests/models.rs`
adds 13 production-interface checks for top-level first-wins lookup, raw AST
immutability, missing/wrong families, model/node namespace collisions, first raw
versus rounded/applied levels, unsupported backends, nonfinite/range/cache errors,
ordered setters/provenance, diode/context defaults and atomic circuit failures.
A module doctest demonstrates the resolver/typed diode API.

The fifth parser C oracle queries `conformance/parser/passive_models.cir`,
checking R/C/L pre-/post-model scalar precedence, forward references, omitted
values and independent C geometry expectations. The new device oracle queries
`model_schemas.cir`, checking first model declarations, bounded diode defaults and
explicit setters, Celsius/Kelvin conversion, repeated diode integer setters and
first BJT/MOS selector choice. Device-query bounds are `1e-12` relative plus
`1e-24` absolute; solver/golden tolerances are unchanged. C ground/model collision
parity, nonlinear equations and advanced/scoped models remain unproven.

Local validation: **282 passed, 0 failed, 7 opt-in C tests ignored** on stable
(rustc 1.99.0) and Rust 1.89.0. All **7** opt-in tests also passed separately
against the read-only local ngspice-47+ binary. Formatting, all-target workspace
Clippy on both toolchains, warning-free rustdoc and `git diff --check` passed.
Default and selected golden verification stood at three supported/five excluded
fixtures at this slice; no goldens were recaptured or changed.

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo +1.89.0 test --workspace --locked
cargo +1.89.0 clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
NGSPICE_BIN=/absolute/path/to/ngspice cargo test --workspace --locked -- --ignored
cargo xtask golden verify
cargo xtask golden verify --netlist rc_lowpass_ac
```

These initial schemas did **not** enable passive or D/Q/M factories, and
parsing/validation is not nonlinear simulation parity. See
[MODEL_SCHEMAS.md](MODEL_SCHEMAS.md) for API contracts, bounded level policy and C
references; #19 now adds the explicitly bounded passive support below.

## Bounded passive elaboration verification

The current checkout implements #19 on merged PR #51 (#11/#17), pending merge.
`spice-devices/tests/passive_models.rs` adds **12** checks for model/instance
precedence, raw AST immutability, R sheet/C area-perimeter formulas, missing or
invalid geometry, coefficient overrides, positive multiplicity/scale, sign and
finite/range/overflow errors, explicit unsupported setters, real stamping,
initial-condition retention and atomic failure across every circuit namespace.
`spice-analysis/tests/passive_models.rs` adds **7** checks comparing model/literal
DC, source sweeps and complex AC; repeated default/nondefault TEMP/TNOM runs;
explicit TEMP/TNOM overrides; runtime errors; contextual RC BDF against its
analytic step; and logarithmic-AC endpoint arithmetic. A new module doctest
exercises contextual passive assembly.

Two opt-in tests in `spice-analysis/tests/c_passive_models.rs` compare effective
values and production DC/complex AC against live C at both 27/27 and 77/22
Celsius. `conformance/parser/passive_elaboration.cir` exercises R/C geometry,
model/default/instance precedence, repeated setters, TC1/TC2, scale and
multiplicity. Query comparisons use **1e-12 relative + 1e-24 absolute**. Production
DC retains **1e-12 + 1e-15** and AC **1e-10 + 1e-12**; analytic BDF retains
**2e-5 V**. No comparison bounds or committed goldens were changed.

Live C exposed that capacitor DEFL is not applied to instances; it is now an
explicit gap and geometry requires instance L. It also exposed an existing AC
logarithmic integer-span roundoff bug that omitted the endpoint. A bounded
floating-arithmetic snap repairs the grid count; integer/noninteger regressions
and C AC checks pass without relaxing value tolerances.

Local validation: **302 passed, 0 failed, 9 opt-in C tests ignored** on stable
Rust 1.99.0 and MSRV 1.89.0. All **9** opt-in C tests also passed separately against
the read-only local ngspice-47+ binary. Both toolchains' all-target Clippy,
formatting, warning-free rustdoc, the RC example and `git diff --check` pass.
Default golden verification stood at three linear fixtures with five
explicit exclusions at this slice; selected AC verification reports one verified fixture.
Neither is full corpus parity.

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo +1.89.0 test --workspace --locked
cargo +1.89.0 clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
NGSPICE_BIN=/absolute/path/to/ngspice cargo test --workspace --locked -- --ignored
cargo xtask golden verify
cargo xtask golden verify --netlist rc_lowpass_ac
cargo run -p spice-analysis --example rc_diffsol --locked
```

[PASSIVE_MODELS.md](PASSIVE_MODELS.md) lists the exhaustive setter/units/default/
formula table and deliberate gaps. Coil geometry, advanced passive forms, D/Q/M
arithmetic, IC/uic and SPICE trap/Gear parity remain unimplemented.

## Bounded waveforms, flags and IC syntax (#8 / #10)

`waveform_parser.rs` adds eight production-parser regressions for PULSE omissions,
PWL pairing, mixed/duplicate DC/AC/waveform setters, leading DC precedence,
byte-column continuation positions, finite/shape/delimiter errors and bounded
work. `rc_transient` unlocked the seventh fixture; #12 now enables all eight.
`flags_ic_parser.rs` adds nine regressions covering the exhaustive bare-flag
inventory, base tokens versus model tail flags, Q 1–2/M 1–3 IC arities,
component names/positions, scalar/vector duplicate order and leading area,
malformed/overflow/advanced forms and first-error/.end behavior.

#9 adds `spice-devices/tests/waveforms.rs` (deck binding, C defaults, limits,
merged lazy breakpoints), `spice-analysis/tests/source_waveforms.rs` (parsed
PWL/PULSE decks through diffsol BDF, jump sampling, budgets) and opt-in
`parsed_pulse_rc_matches_c_on_requested_samples` / `parsed_pwl_rc_matches_c_on_requested_samples`
in `c_linear_reference.rs`.

`spice-devices/tests/parser_setters.rs` adds three tests proving invalid waveform and
nonlinear factory failures are atomic, that flags/IC vectors do not enable
initialization, and that scalar factories/schemas reject non-scalar AST kinds
even when forged with valid numeric text. Device API waveforms are unchanged.

Two new ignored oracles in `c_reference.rs` use `source_waveforms.cir` and
`flags_ic.cir`: C coefficient vectors, preserved PULSE field omissions, last
waveform/DC/AC setters, D/Q OFF, scalar/vector IC overrides and partial IC
fallthrough. Model family flags and MOS1 OFF are input-only in C; successful
setup checks those forms, not nonexistent scalar queries. No rawfile goldens or
solver tolerances changed. See [FRONTEND_VALUES.md](FRONTEND_VALUES.md) for exact
syntax, intentional stricter punctuation and observed C preprocessing limits.

```sh
NGSPICE_BIN=/absolute/path/to/ngspice cargo test -p spice-netlist --test c_reference --locked -- --ignored
```

Local validation: **322 passed, 0 failed, 11 opt-in C tests ignored** on stable
Rust 1.99.0 and MSRV 1.89.0. All **11** opt-in C tests passed separately, including
all seven parser probes. Both toolchains' all-target Clippy, formatting,
warning-free rustdoc and `git diff --check` pass. `cargo xtask golden verify`
remains three verified/five explicitly unsupported fixtures at this slice; no goldens changed.

These are syntax/setup comparisons, not Rust waveform evaluation or nonlinear
simulation. Scoped/source syntax is implemented below. (Historical note: the
expressions, evaluation, options/globals, serialization, snapshots and the
eight-fixture round-trip gate were completed later, #14-#16, #20-#22.)

## Scoped/source syntax (#12 / #13)

`subcircuits.rs` and `sources.rs` test ordered nested scope storage, forward
model names, X/formal textual parameters, structural diagnostics, source-relative
includes/library selections, include-chain/section-boundary provenance,
canonical cycles/symlink aliases, repeated/diamond includes, depth/file/byte/card
limits and ordered failures/termination. `spice-rs parse` resolves the committed
multi-file `conformance/parser/sources/main.cir` probe and all eight rawfile
fixtures; `spice-devices/tests/structure_parser.rs` explicitly rejects simulation
and checks atomic X factory failures. The new textual parameter kind is also
rejected by scalar consumers. See [FRONTEND_STRUCTURE.md](FRONTEND_STRUCTURE.md).

An eighth opt-in parser oracle copies that multi-file source probe into a scratch
directory and queries C's selected model RSH after setup. It checks accepted
source/subcircuit syntax and library selection, **not Rust flattening or
simulation**. No committed rawfile goldens or solver tolerances changed.

Local validation for this slice: **342 passed, 0 failed, 12 opt-in tests ignored**
on stable and Rust 1.89.0; both all-target Clippy checks and formatting pass.
All **12 opt-in C tests** pass separately on stable, including all **8 parser
probes** and the new source probe. Warning-free rustdoc and `git diff --check` pass. `golden verify` still
reports three verified/five unsupported fixtures at this slice, naming
subcircuit **flattening/elaboration**, not parsing, as the blocker; #18 later
removed it (the fixture now verifies and `EXCLUDED` is empty).

## Petgraph topology verification

`spice-devices::Circuit::topology()` is exercised by nine additional circuit
regressions: ground/unused nodes, separate node/device/row namespaces, port
order and repeated multiport edges, disconnected structural components,
zero-port devices, snapshot rebuilding after mutation, duplicate/dangling
mutations with unchanged numbering on failure, and deterministic deck order.
Existing device/container regressions remain in place.

`crates/spice-maths/tests/mna_topology.rs` now calls the production
`SparseMatrix::coupling_graph()` instead of a hardcoded test-only graph builder.
Eight checks cover structural blocks, diagonal-only/empty rows, one ordinary
row-0 unknown, the empty matrix, duplicate cancellations and input immutability,
asymmetric/opposite-sign entries, fresh graphs after stamps/clear, and invalid
rectangular shapes. Connectivity uses petgraph algorithms, not custom traversal.

These tests prove structural projection correctness, **not** DC ground-path
validity, model/analysis-specific topology rules or numerical nonsingularity.
Graph extraction has no finite-value validation and does not implement a solver.
The parser/C-oracle and golden checks remain unchanged.

## Production linear-engine verification

Main now contains a bounded linear engine. Production `.op` and complex `.ac`
results are compared with committed C goldens by variable name, not vector order:
DC uses `1e-12` relative plus `1e-15` absolute near zero; AC uses `1e-10` relative
plus `1e-12` absolute. Analytic RC/RL/RLC, floating-capacitor and coupled-
capacitance transient tests, and opt-in live C Pwl comparisons for the RC,
floating-capacitor and coupled-capacitance decks (`c_linear_reference.rs`, `2e-5` V
on the requested 100 µs grid through the 1 ms/1.01 ms knots), use common physical
output grids rather than identical adaptive internal timesteps. See
[DIFFSOL_FAER_IMPLEMENTATION.md](DIFFSOL_FAER_IMPLEMENTATION.md) for test coverage,
recorded stable/MSRV validation, error bounds and numerical restrictions.

These tests exercise Rust production interfaces, unlike `golden check`, which
only checks reproducibility of captured C output. Topology and rawfile regression
tests remain necessary but do not alone establish simulation correctness.

## Rust-engine golden verification

```sh
cargo xtask golden verify
cargo xtask golden verify --netlist rc_lowpass_ac
```

The default verifies **26 fixtures** through `Parser::parse_file`,
`RunConfig::from_netlist` (so deck `.options`, for example `method=gear`, reach the
driver exactly as in an ordinary run), `Circuit::from_netlist` and the production
analysis runner: `rc_divider`, `rlc_series` and the flattened `subckt_divider`
(`.op`), `rc_lowpass_ac` and `rlc_series_ac` (complex `.ac`), the transients
`rc_transient`, `rl_pulse_tran`, `rc_gear_tran`, `rc_pwl_tran`, `rlc_series_tran`,
`rlc_series_gear_tran`, `floating_cap_tran` and `coupled_cap_tran`, the
initialized-state transients `rc_ic_uic_tran`, `rlc_ic_uic_tran`,
`rc_ic_node_tran` and `floating_cap_ic_tran`, and the M4 nonlinear decks
`diode_dc`, `bjt_ce`, `mos_inverter`, `m4_diode_ac`, `m4_bjt_ac`, `m4_mos1_ac`,
`m4_diode_tran`, `m4_bjt_tran` and `m4_mos1_tran`. It reports **no unsupported
fixtures** — `EXCLUDED` is empty and `subckt_divider` verifies through its own
production path. A requested unsupported
fixture fails, never silently skips. Names are case-insensitive and an optional
`.cir` suffix is accepted. Unknown fixtures/options, missing input/goldens,
unregistered new fixtures and missing default supported decks fail explicitly.
Verify accepts only `--netlist`; it neither locates/executes C nor writes fixtures,
goldens or scratch output. Capture/check/list retain their existing behavior.

The extension registry is `xtask/src/verify.rs` (`SUPPORTED`/`EXCLUDED`). A
`SUPPORTED` entry may list `Variant`s: additional Rust-only runs of the same deck
against the *same* golden whose extra request tokens are appended to the deck's
own (today `backend=diffsol method=bdf`, a syntax C rejects), each with its own
tolerance. Variants never edit decks or goldens, and are refused by `RunConfig` if
the deck selects `method=trap/gear` (no silent downgrade). Add an
analysis kind, axis identity and comparison policy only after demonstrating
production support, not merely parser support. Exactly one deck analysis and
one C plot are required. `xtask/src/compare.rs` centralizes metadata, shape,
finite-value and numerical checks: plot name/flags, point count, unique variable
names, units and real-vector flags must match. Internal plot IDs and rawfile
Title/Date/Command headers are intentionally not numerical comparisons.
Columns match case-insensitively **by name**, independent of C/Rust ordering.
The frequency axis must be real, positive, strictly increasing and numerically
match at every sample; no interpolation or transient resampling occurs.

Each real/imaginary component satisfies
`|Rust - C| <= relative * |C| + absolute`: DC retains **1e-12 + 1e-15**, and AC
**1e-10 + 1e-12**. The absolute term bounds near-zero currents/cancellation;
relative terms retain the existing production test bounds, not a new looser
policy. Failures report first and worst component mismatches (point, variable,
values, error and bound). Counts explicitly describe bounded coverage, not full
corpus parity.

Ordinary xtask tests cover reordered/renamed/dropped/duplicate columns,
real/imaginary and zero perturbations, metadata/axis/shape/nonfinite errors,
corrupt/missing/multiple goldens, parser/elaboration/numerical failures, missing
registry coverage and process exit statuses. Temporary copies are used for
corruption tests; committed fixtures are never rewritten. Process tests select
an unavailable `NGSPICE_BIN` to verify that C is unnecessary.

## Transient comparison tooling (#48 item 1)

`xtask/src/tran.rs` is the event-aware comparator for the M3 transient exit gate.
All twelve transient fixtures (`rc_transient`, seven gate decks, four initialized-state decks) are
registered in `golden verify` against their committed goldens (see above). The opt-in live comparisons in
`crates/spice-analysis/tests/c_companion_reference.rs` cover the PULSE/PWL RC and
series RLC decks for trap and Gear. C rejects `backend=diffsol method=bdf` tokens on `.tran`
("Cannot compute substitute"), so the BDF backend is compared with the committed
goldens only through the registry's Rust-only `Variant`s (same deck text plus
BDF tokens) and otherwise with analytic solutions.

* Timestep sequences are never compared. Both plots are evaluated on a shared
  grid `0, step, ..., stop` (`tran::Grid`); linear interpolation is used only
  between two neighbouring samples of one plot with no breakpoint between them,
  otherwise the comparison fails (it never passes by smoothing over an event).
* Breakpoints come from the deck AST (`tran::breakpoints`): PWL knot times and
  PULSE `TD + n*PER + {0, TR, TR+PW, TR+PW+TF}` with the C `VSRCaccept`
  defaults, never from the data. At a breakpoint the left and right limits are
  compared separately: two samples at one instant are (left, right); one sample
  serves as both limits; no sample at a breakpoint is an error (ngspice lands a
  step on every breakpoint; the companion driver emits one, the diffsol BDF
  requested-grid output only on-grid events).
* `uic` runs: C writes no `t = 0` row (the first row is the first accepted step) and
  adds a breakpoint at the `.tran` step. `tran::Grid` has a `start` (0 normally);
  for a `uic` deck `verify.rs` passes the golden's first time and **both** plots must
  begin exactly there (the instant is compared as an ordinary sample; nothing before
  it is interpolated or extrapolated). The step breakpoint needs no declaration:
  it is not a source corner, both plots carry a sample there, and interpolation
  across it is harmless (the data are smooth).
* End time (and the start) must match `grid.stop`/`grid.start`; missing/extra
  variables, unit/metadata mismatch, non-real data, nonfinite values, decreasing
  time and repeated times away from declared breakpoints are errors.
* `compare::TRAN`: relative 1e-3 (ngspice `reltol`) plus 1e-6 V / 1e-12 A
  (`vntol` / `abstol`) by signal unit. These are the simulator's default accuracy
  floors, not values fitted to a fixture. Every companion (trap/Gear-2) run in the
  registry uses it unchanged; measured worst errors against the committed goldens
  are 0.000 of the bound (the port reproduces C's step sequence).
* `compare::TRAN_RESTART` adds `reltol * max|C signal|` to that bound. It is used
  **only** for Rust-only BDF variants of decks with source corners
  (`rl_pulse_tran`, `rc_pwl_tran`, `rlc_series_tran`, `coupled_cap_tran`). Reason: C restarts its
  trapezoidal rule with a backward-Euler step after every breakpoint (`dctran.c`),
  a first-order local error. Against exact solutions (state-space matrix
  exponential in `m3_gate.rs`, independently RK4) C and the companion driver are off
  by up to 2e-4 (RLC) / 4% of `v(out)` ten microseconds after a PWL corner on a 1 V
  signal, while BDF is exact to ~1e-7. ngspice's own truncation control bounds error
  relative to the peak charge, not the instantaneous value, so a pointwise
  relative bound is stricter than C guarantees at small values. The more accurate
  solver is therefore compared with `reltol` of the signal peak; the worst measured
  ratios are 0.38 (RLC), 0.10 (RC PWL), 0.06 (coupled), 0.05 (RL) of that bound.
  `rl_pulse_tran` passes the pointwise `TRAN` bound too, but at 0.94 of it, so it
  uses the same policy as its siblings. `floating_cap_tran` keeps the pointwise
  `TRAN` bound (0.11).
* Registry design guard: BDF variants require every source corner on the `.tran`
  output grid (the BDF backend emits the requested grid, and the comparator demands
  a sample at each breakpoint) and a stop time that is an exact multiple of `tstep`
  (otherwise the BDF grid ends with a duplicate sample one ulp before the stop,
  which the comparator rightly rejects as a non-breakpoint repeat).

Tests use only synthetic and committed data (no C): grid alignment on unrelated
timesteps, in-segment interpolation, refusal across breakpoints, jump left/right
limits, end-time mismatch, missing signals, nonfinite data, AST breakpoints, and
an end-to-end diffsol BDF RC PWL ramp against its analytic response (worst
error 4e-5 of the bound).

## Initialized-state fixtures (#27, #48)

Four decks with C goldens captured deliberately (one `cargo xtask golden capture
--netlist <name>` each; no existing golden recaptured, ~50-150 KB each) and
registered with `compare::TRAN` unchanged. Worst error against C is 0.000 of the
bound for all four (the port reproduces C's step sequence, including the `uic`
first step and step breakpoint).

| Deck | Initial state | Exercises |
| --- | --- | --- |
| `rc_ic_uic_tran` | `uic`, `c1 ic=2`, 0 V source | RC discharge `2 exp(-t/1 ms)`, no `t = 0` row |
| `rlc_ic_uic_tran` | `uic`, `l1 ic=20m`, `c1 ic=1`, 0 V source | underdamped free decay (zeta 0.158) |
| `rc_ic_node_tran` | `.ic v(out)=0.25`, no `uic`, 1 V source | constrained bias row at `t = 0`, then release |
| `floating_cap_ic_tran` | `uic`, `c1 ic=2` between floating a/b, ramp drive | plate charge changes only by the current through r1 |

The diffsol BDF backend deliberately rejects `.ic`, `uic` and instance `ic=`, so these
decks have **no BDF variants**; `bdf_variants_of_initialized_state_decks_are_rejected_explicitly`
(xtask) and `the_diffsol_bdf_backend_rejects_every_initialized_state_deck_explicitly`
(`m3_gate.rs`) assert the explicit error.

Gate checks in `m3_gate.rs` (Rust and the C golden against the same exact solution
from `t = 0`; budgets at most twice the measurement, relative to the device scale):

| Deck | worst error / scale | budget |
| --- | --- | --- |
| `rc_ic_uic_tran` | 5.9e-6 | 1.2e-5 |
| `rlc_ic_uic_tran` | 2.5e-4 | 4.9e-4 |
| `rc_ic_node_tran` | 2.3e-6 | 4.6e-6 |
| `floating_cap_ic_tran` | 1.5e-6 | 3e-6 |

The floating capacitor's plate charge `C (va - vb)` moves only by the charge through
r1: residual 1.39e-6 of the peak charge (budget 2.8e-6), the first row is within
5.0e-5 of `C ic` (one 0.1 us backward-Euler step of decay; budget 1e-4) and KCL
holds to 1.1e-12 (budget 1e-9). The `.ic` deck's `t = 0` row is `v(out) = 0.25`,
`i(v1) = -0.75 mA`; the same deck without the `.ic` card stays at its 1 V operating
point, so the constraint (not the circuit) set the state, and it is released
afterwards (`v(out) = 1 - 0.75 e^(-t/tau)`). `uic` runs have a first row at
0 < t < tstep/10 equal for Rust and C, a sample at the `.tran` step and the right
breakpoint counts.

## M3 exit-gate analytic and conservation checks (#48)

`crates/spice-analysis/tests/m3_gate.rs` runs the committed gate decks through
`Parser` -> `RunConfig` -> `companion_transient`/`runner` (no C needed) and checks
them against exact closed forms (a state-space model advanced with a matrix
exponential over the deck's piecewise-linear drive; the same model judges the
committed C goldens), conservation laws at accepted points, and production-API
semantics. Budgets are measured physical error limits relative to the device scale
(at most 2x the measurement; Rust and C agree to 1e-9 of their own error):

| Deck | method | worst error / scale | budget |
| --- | --- | --- | --- |
| `rl_pulse_tran` | trap | 4.8e-5 | 1e-4 |
| `rc_gear_tran` | Gear-2 | 6.8e-5 | 1.4e-4 |
| `rc_pwl_tran` | trap | 7.1e-6 | 1.5e-5 |
| `rlc_series_tran` | trap | 1.9e-4 | 4e-4 |
| `rlc_series_gear_tran` | Gear-2 | 7.5e-4 | 1.5e-3 |
| `floating_cap_tran` | trap | 1.8e-6 | 4e-6 |
| `coupled_cap_tran` | trap | 3.7e-6 | 8e-6 |
| all of the above | explicit BDF | 1.3e-7 to 3.6e-7 | 7e-7 |

Halving `tmax` on the RLC decks reduces the error by 3.98 then 3.99 (order 2, trap
and Gear). KCL holds to rounding at every accepted point (floating and coupled
networks included). Capacitor charge is conserved to 8e-8 to 1.6e-7 of the peak
plate charge (floating and coupled), energy balance `supplied = stored +
dissipated` to 1.3e-5 to 1.6e-4 of the peak supplied energy; BDF conserves the
floating charge to 2.1e-6 and KCL to 1.6e-8. The `.ac` sweep matches
`H = 1/(1 - w^2 LC + jwRC)`, KCL and KVL to 1e-9. Production-API tests cover: only
accepted points in the output, exact landing on `tstop`, default and explicit
`maxstep`, a sample at every source breakpoint, breakpoint counts, rejected steps
(more rejections at tighter `reltol`, none in the output), work-limit
(`maxsteps`) and minimum-step failures and unsupported `maxord`.

Findings recorded by the gate (no driver change made): deck PWL with a repeated
time is rejected at parse time ("strictly increasing"), so a true source jump is
only reachable through the device API (`companion_transient.rs`); with an extreme
`trtol` on the RLC deck the row-equilibrated companion solves stay well conditioned
and the run ends at the `maxsteps` work limit rather than "timestep too small" (an
explicit failure either way). The BDF requested grid used to end with a duplicate
sample one ulp beside `tstop` when `tstop` was not an exact binary multiple of
`tstep` (e.g. `.tran 50u 3m`); it now ends with exactly one `tstop` sample
(`source_waveforms.rs`).

**Still blocked, not claimed:** higher-index source constraints (#29), nonlinear
charge and devices (M4), orders above 2, mutual inductors and
nonlinear device initial conditions, and general MNA DAEs: only the index-one
structures demonstrated above are covered.

## Not yet verified

Full corpus simulation, nonlinear D/Q/M arithmetic, trap/Gear transient parity
beyond the linear RC/RLC decks above, general DAEs and the remaining source
waveforms
are not established by the bounded linear implementation; subcircuit elaboration
and parameter scoping are covered by the #18 gate ([SUBCIRCUITS.md](SUBCIRCUITS.md)).
Track those remaining
gates in the central [TODO.md](../../TODO.md); do not claim full SPICE parity.
