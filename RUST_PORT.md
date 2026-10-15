# RUST_PORT.md — a Rust port of ngspice

A single-crate Cargo workspace for a from-scratch Rust implementation of ngspice. The
optional development
setup includes C-reference and solver-integration worktrees; the public standalone
repository contains only the Rust port and its conformance data. These Git
histories differ, and the local development mainline may lag public main.

**Nothing in the C sources is modified or replaced.** ngspice is the reference
implementation, inspected during porting and queried out of process. Verification
is incremental; see [`docs/port/VERIFICATION.md`](docs/port/VERIFICATION.md) for
what is proven and what is still missing.

The Rust port is a **derivative work of ngspice** and is distributed under the
same Modified BSD license (see [`COPYING`](COPYING)).

## Status

**Current status (after M10 spill slice H1a, 2026-10-15):** `main` is the
single-crate `ngspice-rs` 0.4.0 package. Since the sections below were written,
M6–M9 landed and M10 waves 0–3 added JFET levels 1/2, MESFET level 1, lossless
`T`, lossy `O`/LTRA and `U`/URC lines and MOS levels 2/3/6/9, the spill slice H1a
added MESA level 2, and faer/diffsol gave way to the native SPARSE 1.3 solver.
Current evidence: `cargo test --workspace --locked` reports **1427 passed / 0
failed / 150 ignored** and `cargo xtask golden verify` **153 verified / 1
unsupported / 0 failures**.
[TODO.md](TODO.md) is the current checklist; the milestone narrative below is
retained as a record of the earlier checkouts, and its "in progress" or "pending
merge" language no longer describes `main`.

**Single-crate packaging:** the port is one package, `ngspice-rs`, not six crates
(`spice-core`, `spice-netlist`, `spice-maths`, `spice-devices`, `spice-analysis`,
`spice-cli`). Those boundaries are now the module tree under `src/`
(`primitives`, `netlist`, `maths`, `devices`, `analysis`, `cli`), the CLI binary is
`src/bin/spice-rs.rs` and integration tests live in `tests/`. That layout-only
step changed no production behavior: at consolidation it reported **1051 passed /
0 failed / 67 ignored**, `golden verify` **59/0/0** and **196** snapshots
unchanged (superseded by the current totals above).
`cargo package` produces a 2.0 MiB `.crate` whose own test suite passes, because
conformance fixtures are package-relative and a downloaded crate is therefore
self-testable. Layering is now a review-enforced convention rather than a
compiler-enforced one ([ARCHITECTURE.md](docs/port/ARCHITECTURE.md)), and
crates.io release automation is in place
(`.github/workflows/publish.yml`, see [RELEASING.md](docs/port/RELEASING.md));
the last published release is 0.4.0, which predates M10.

**Bounded numerical follow-up — historical (faer era):** the items below belong
to the solver stack that M10 wave 0.5 removed; they describe no current code. #46
adds explicit maths-library
`EquilibratedDenseLu`/`EquilibratedSparseLu`/`EquilibratedComplexLu` wrappers with
bounded transforms and original-unit residual checks
([EQUILIBRATION.md](docs/port/EQUILIBRATION.md)); simulation defaults are unchanged.
#47 delivered a backend audit, guard regressions and measured example-only batching,
retaining the then-current rank policy ([SPARSE_RANK_DIAGNOSTICS.md](docs/port/SPARSE_RANK_DIAGNOSTICS.md)).
Its aggregate-certification proof limitation stays tracked in #68. #29 supplied a
tested constrained-RLC prototype/ADR
([HIGHER_INDEX_DAE_ADR.md](docs/port/HIGHER_INDEX_DAE_ADR.md)), never production
higher-index support, with separate enabling gates #69–#72. The native SPARSE 1.3
port supersedes all of it: C's own error policy, no rank probe or residual check
([NATIVE_SOLVER.md](docs/port/NATIVE_SOLVER.md)); no deck consumes #68–#72.
Historical validation at the time: **824 passed / 0 failed / 37 ignored**;
golden verify **26/0/0**, **37 live-C** checks passed.

**Branch-local M4 update:** bounded diode/Ebers-Moll BJT/MOS1 DC, AC and
charge-companion transient, reusable Newton/continuation and typed nested
source/temperature sweeps are implemented. Local #34/#35 follow-ups add bounded
configurable DC policies/reports and scalar-resistor sweeps (including supported
model-backed resistance); see [DC_CONTINUATION.md](docs/port/DC_CONTINUATION.md) and
[DC_SWEEPS.md](docs/port/DC_SWEEPS.md). The follow-ups are not yet published. See
[M4_NONLINEAR.md](docs/port/M4_NONLINEAR.md) for exact allowlists, deliberate
rejections and local #41 evidence. Historical scaffold notes below do not widen
this demonstrated subset or imply full SPICE parity.

**M1 in progress; M1a and basic model/D/Q/M syntax are implemented.**
The workspace compiles,
the test suite passes, and the verification harness captures golden data from
the C binary. The parser builds ASTs for scalar R/C/L, DC/AC V/I, scalar
D/BJT/MOS/R/C/L model cards and bounded D/Q/M instances. Q accepts three ports
plus optional substrate; M accepts drain/gate/source/bulk and scalar geometry.
Q/M require a declaration in their scope or an ancestor before `.end` (forward definitions
work); the declaration index disambiguates ports, not model backends.
PR #51 merged declared-model passive syntax and initial model infrastructure
(#11/#17). This checkout additionally implements bounded model-backed R/C/L
geometry/temperature/scale/multiplicity (#19, pending merge); see
[PASSIVE_MODELS.md](docs/port/PASSIVE_MODELS.md). Numeric PULSE/PWL and bounded
flags/Q/M IC vectors now parse (#8/#10); see [FRONTEND_VALUES.md](docs/port/FRONTEND_VALUES.md).
Scoped subcircuits/X and source-relative includes/libraries parse (#12/#13), and
`X` instances now elaborate through the production entry points (#18) with
hierarchical names, scoped parameters/models and `.global`; see
[SUBCIRCUITS.md](docs/port/SUBCIRCUITS.md) and
[FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md). Advanced passive forms
and the remaining numparam surface remain unported. D/Q/M AST/schema
success alone does not imply the requested physics is implemented: M4's bounded
allowlists are validated by the model-aware factories.
**M5 wave 2** adds bounded `.save`/`.print` output selection (#42, [OUTPUT_SELECTION.md](docs/port/OUTPUT_SELECTION.md))
and bounded `.measure`/`.meas` measurements (#43, [MEASURE.md](docs/port/MEASURE.md)):
`.save`/`.print` cards parse into positioned typed requests and project the full
plot into the written rawfile in C's `dbs` order with first-wins dedup (`.print`
also renders a text table), a bounded `.measure` subset evaluates over the full
plot before output selection narrows the rawfile, and unresolvable requests fail
before anything is published. Bounded `.four` (#44,
[FOURIER.md](docs/port/FOURIER.md)) now evaluates the final complete transient
period over the full plot with physical-grid quadrature, DC/peak amplitude,
window-referenced phase in radians and THD (1–100 harmonics). All six bounded
M5 deliverables are implemented; `.plot`, interactive Fourier commands and the
documented extended forms remain unported.
Main implements scalar R/C/L/V/I elaboration and equations, real/complex LU
through a Rust port of SPARSE 1.3, linear `.op`, single-source `.dc`, complex
`.ac`, and the adaptive trapezoidal / Gear-2 companion `.tran` driver, the only
transient backend (the diffsol BDF backend was removed in M10 wave 0.5).
This worktree extends the companion/DC/AC paths with M4's bounded D/Q/M equations.
Physics outside the M4 allowlists, general DAEs and remaining usability work
remain unimplemented. Unsupported cases
fail explicitly; pending ports use
[`SpiceError::NotYetPorted`](src/primitives/error.rs) naming a C reference.
See [TODO.md](TODO.md) for the central checklist and
[NATIVE_SOLVER.md](docs/port/NATIVE_SOLVER.md) for production solver APIs,
numerical policies and limits.

What already works for real:

| Area | Crate | Notes |
| --- | --- | --- |
| SPICE numeric literals and scale factors | `primitives` | `INPevaluate()` table from `src/spicelib/parser/inpeval.c` |
| Node table, ground aliasing | `primitives` | `inp_fix_gnd_name()` from `src/frontend/inpcom.c` |
| Deck loading: title, continuation, comments | `netlist` | `inp_stripcomments_line()`, `inp_readall()` |
| Card tokenizer and `.command` classification | `netlist` | `inppas2.c` / `inp2dot.c` dispatch |
| Winnow semantic parser | `netlist` | borrowed token-stream combinators; scalar R/C/L, DC/AC V/I, models, bounded D/Q/M flags/IC vectors, PULSE/PWL, opaque analyses, scoped subcircuits/X, resolved includes/libraries; 8/8 fixture parses, not simulation |
| Opt-in live parser oracle | `netlist` tests | compares scalar AST parameters and Q/M terminal order with live C queries |
| Real/complex MNA storage and LU | `maths` | SPARSE 1.3 port (`sparse13`) with C's pivoting and singular-matrix policy, owned factors |
| Model resolver and initial scalar schemas | `devices` | first-declaration lookup, family/level checks, diode input projection plus bounded M4 model-aware D/Q/M factories |
| Bounded model-backed passives | `devices`, `analysis` | R sheet/C area-perimeter geometry, model L, contextual TC1/TC2, scale/multiplicity; no coil geometry |
| Scalar R/C/L/V/I elaboration and equations | `devices` | ground elimination, branch binding, immutable linear operators |
| Linear DC/AC and bounded transient | `analysis` | `.op`, single-source `.dc`, complex `.ac`, trap/Gear-2 companion `.tran`; linear only |
| Petgraph topology APIs | `devices`, `maths` | circuit incidence/per-port edges and assembled matrix-row coupling; no DC-path/solvability claim |
| Rawfile read *and* write: ASCII and binary | `analysis` | `src/frontend/rawfile.c` layout; binary real/complex read/write with explicit byte order, validated payload lengths and rejected variants documented in [RAWFILES.md](docs/port/RAWFILES.md) |
| Output selection (`.save`/`.print`) | `netlist`, `analysis`, `cli` | bounded typed request parsing and projection into the written rawfile in C `dbs` order with first-wins dedup; `.print` text table; unresolvable/unsupported requests fail before publishing; `.plot` unported ([OUTPUT_SELECTION.md](docs/port/OUTPUT_SELECTION.md)) |
| Measurements (`.measure`/`.meas`) | `netlist`, `analysis`, `cli` | bounded `FIND <operand> AT=`, `MIN`/`MAX`/`AVG`/`RMS`/`INTEG` and `TRIG … TARG …` subset evaluated over the full plot before output selection narrows it; a failing card fails the run ([MEASURE.md](docs/port/MEASURE.md)) |
| Conformance fixtures and goldens | `conformance/`, `xtask` | 20 decks (original 8, 8 M3 gate decks, 4 initialized-state decks), captured from `ngspice-47+` |
| Golden-data capture and drift check | `xtask` | drives the C `ngspice` binary |
| Rust-engine numerical verify | `xtask` | 26 verified fixtures (op, AC, trap/Gear-2 transients, `.ic`/`uic`/`ic=` transients, nonlinear and flattened-subcircuit decks); no exclusions, no C invocation |

## Quick start

```sh
cargo build                          # build the workspace
cargo test                           # run the test suite, including conformance tests
cargo xtask help                     # automation commands
cargo xtask golden list              # what the captured goldens contain
cargo xtask golden verify            # Rust vs committed C data; bounded coverage
cargo xtask golden check             # re-run C ngspice, diff against the goldens
cargo xtask ci                       # fmt --check + clippy -D warnings + test
```

The workspace uses Rust edition 2024 and MSRV **1.89**. The sparse solver is a
port of SPARSE 1.3 (Berkeley notice), not a translation of LGPL KLU; there are
no native solver dependencies. `cargo test` needs Rust and the locked registry dependencies; the goldens are
committed data, so no C toolchain is required. Run `cargo fetch --locked` once
before `cargo test --workspace --locked --offline` for offline use.
`cargo xtask golden capture` needs a built C `ngspice` binary. Point at one with
`--ngspice <path>` or `NGSPICE_BIN=<path>`; otherwise it looks for
`build/src/ngspice` — which only exists if you are working inside an ngspice
checkout that has been built — and then for `ngspice` on `PATH`.

The CLI is `spice-rs`. It loads and tokenizes decks, classifies cards, and can
build semantic netlists for supported syntax. `spice-rs parse` succeeds on
`rc_divider`, `rc_lowpass_ac`, `rlc_series`, `diode_dc`, `bjt_ce` and
`mos_inverter`, `rc_transient` and `subckt_divider` (all original eight fixtures; the later M3 gate decks parse and simulate as well).
Parsing succeeds for all eight (the M1 round-trip gate is
`tests/m1_gate.rs`, #22). **`spice-rs simulate --output
<path> <deck>` runs the deck's single analysis through the production runner and
writes an ASCII rawfile** ([CLI.md](docs/port/CLI.md)); it honours the bounded
`.save`/`.print` output selection ([OUTPUT_SELECTION.md](docs/port/OUTPUT_SELECTION.md))
and reports the bounded `.measure`/`.meas` measurements ([MEASURE.md](docs/port/MEASURE.md)). The same APIs remain
available directly through `ngspice_rs::analysis::runner`.

```sh
cargo run -p ngspice-rs -- conformance/netlists/rc_divider.cir    # deck summary
cargo run -p ngspice-rs -- cards conformance/netlists/bjt_ce.cir  # classifications
cargo run -p ngspice-rs -- parse conformance/netlists/rc_divider.cir # real AST
cargo run -p ngspice-rs -- devices                                # device coverage
cargo run -p ngspice-rs -- analyses                              # analysis coverage
```

## Documentation

- [`docs/port/ARCHITECTURE.md`](docs/port/ARCHITECTURE.md) — module layout, dependency direction, design rules
- [`docs/port/MAPPING.md`](docs/port/MAPPING.md) — C source tree → Rust module mapping
- [`docs/port/ROADMAP.md`](docs/port/ROADMAP.md) — milestones and current position
- [`TODO.md`](TODO.md) — central branch-aware checklist, next tasks and completion gates
- [`docs/port/VERIFICATION.md`](docs/port/VERIFICATION.md) — how parity with C is proven

## Development setup

The optional local setup includes `ngspice-rs` (development main with C
references), `ngspice-rs-diffsol-faer` (historical solver-integration
worktree), `ngspice-rs-dist`
(Rust-only publication) and `ngspice_test` (C oracle). Inspect the chosen branch
and status; do not assume these checkouts are synchronized.

The development checkout provides `scripts/publish-rust-only.sh`, exporting
tracked Rust files without C sources/history. Its publisher maps `rust-port` to
public `main` and retains other named branches. Before a whole-tree export,
ensure the source includes all newer public-side changes; current GitHub main
already contains the solver merge. Focused documentation publication must
preserve that implementation and keep the distinct histories separate. A
whole-tree export also carries the package layout, so an export from a checkout
still using the six-crate `crates/spice-*` tree would revert the consolidated
`src/` layout: treat a layout change as whole-tree, never as a cherry-pick.

Nothing in the port depends on that setup: `cargo test` and `cargo xtask ci` run
in any checkout. C capture/check and opt-in live oracles need an upstream
`ngspice` binary, supplied through `NGSPICE_BIN` or `--ngspice`.
