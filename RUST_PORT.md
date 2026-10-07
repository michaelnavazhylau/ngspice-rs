# RUST_PORT.md — a Rust port of ngspice

A Cargo workspace for a from-scratch Rust implementation of ngspice. The optional development
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

**Branch-local M4 update:** bounded diode/Ebers-Moll BJT/MOS1 DC, AC and
charge-companion transient, reusable Newton/continuation and typed nested
source/temperature sweeps are now implemented. See
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
Scoped subcircuits/X and source-relative includes/libraries now parse (#12/#13);
see [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md).
Waveform deck evaluation, subcircuit flattening, expressions and advanced passive forms remain unported. D/Q/M AST/schema
success alone does not imply the requested physics is implemented: M4's bounded
allowlists are validated by the model-aware factories.
Main implements scalar R/C/L/V/I elaboration and equations, real/complex faer
LU, linear `.op`, single-source `.dc`, complex `.ac`, and explicitly selected
an adaptive trapezoidal / Gear-2 companion `.tran` driver (ordinary `.tran`,
linear circuits) and an explicitly selected bounded diffsol BDF transient.
This worktree extends the companion/DC/AC paths with M4's bounded D/Q/M equations.
`.ic`/`uic` on diffsol or nonlinear companion circuits, physics outside the M4
allowlists, general DAEs and a CLI simulation command remain unimplemented. Unsupported cases
fail explicitly; pending ports use
[`SpiceError::NotYetPorted`](crates/spice-core/src/error.rs) naming a C reference.
See [TODO.md](TODO.md) for the central checklist and
[DIFFSOL_FAER_IMPLEMENTATION.md](docs/port/DIFFSOL_FAER_IMPLEMENTATION.md) for
production APIs, numerical policies, recorded validation and limits.

What already works for real:

| Area | Crate | Notes |
| --- | --- | --- |
| SPICE numeric literals and scale factors | `spice-core` | `INPevaluate()` table from `src/spicelib/parser/inpeval.c` |
| Node table, ground aliasing | `spice-core` | `inp_fix_gnd_name()` from `src/frontend/inpcom.c` |
| Deck loading: title, continuation, comments | `spice-netlist` | `inp_stripcomments_line()`, `inp_readall()` |
| Card tokenizer and `.command` classification | `spice-netlist` | `inppas2.c` / `inp2dot.c` dispatch |
| Winnow semantic parser | `spice-netlist` | borrowed token-stream combinators; scalar R/C/L, DC/AC V/I, models, bounded D/Q/M flags/IC vectors, PULSE/PWL, opaque analyses, scoped subcircuits/X, resolved includes/libraries; 8/8 fixture parses, not simulation |
| Opt-in live parser oracle | `spice-netlist` tests | compares scalar AST parameters and Q/M terminal order with live C queries |
| Real/complex MNA storage and LU | `spice-maths` | faer factors, rank/finite/residual diagnostics and owned snapshots |
| Model resolver and initial scalar schemas | `spice-devices` | first-declaration lookup, family/level checks, diode input projection plus bounded M4 model-aware D/Q/M factories |
| Bounded model-backed passives | `spice-devices`, `spice-analysis` | R sheet/C area-perimeter geometry, model L, contextual TC1/TC2, scale/multiplicity; no coil geometry |
| Scalar R/C/L/V/I elaboration and equations | `spice-devices` | ground elimination, branch binding, immutable linear operators |
| Linear DC/AC and bounded transient | `spice-analysis` | `.op`, single-source `.dc`, complex `.ac`, trap/Gear-2 companion `.tran` (ordinary) and explicit diffsol BDF; linear only |
| Petgraph topology APIs | `spice-devices`, `spice-maths` | circuit incidence/per-port edges and assembled matrix-row coupling; no DC-path/solvability claim |
| ASCII rawfile read *and* write | `spice-analysis` | `src/frontend/rawfile.c` layout; known decimal round-trip limitation documented in verification |
| Conformance fixtures and goldens | `conformance/`, `xtask` | 20 decks (original 8, 8 M3 gate decks, 4 initialized-state decks), captured from `ngspice-47+` |
| Golden-data capture and drift check | `xtask` | drives the C `ngspice` binary |
| Rust-engine numerical verify | `xtask` | sixteen supported linear fixtures (op, AC, trap/Gear-2/BDF transients, `.ic`/`uic`/`ic=` transients); four explicit exclusions, no C invocation |

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

The workspace uses Rust edition 2024 and MSRV **1.89**. faer and diffsol are
MIT-licensed backends, not translations of LGPL KLU; SuiteSparse/SUNDIALS are
disabled. `cargo test` needs Rust and the locked registry dependencies; the goldens are
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
This is parsing, not subcircuit flattening or simulation (the M1 round-trip gate is `crates/spice-netlist/tests/m1_gate.rs`, #22). **The CLI does not simulate yet; production simulation
is available through APIs and `cargo run -p spice-analysis --example rc_diffsol`.**

```sh
cargo run -p spice-cli -- conformance/netlists/rc_divider.cir    # deck summary
cargo run -p spice-cli -- cards conformance/netlists/bjt_ce.cir  # classifications
cargo run -p spice-cli -- parse conformance/netlists/rc_divider.cir # real AST
cargo run -p spice-cli -- devices                                # device coverage
cargo run -p spice-cli -- analyses                              # analysis coverage
```

## Documentation

- [`docs/port/ARCHITECTURE.md`](docs/port/ARCHITECTURE.md) — crate layout, dependency direction, design rules
- [`docs/port/MAPPING.md`](docs/port/MAPPING.md) — C source tree → Rust crate mapping
- [`docs/port/ROADMAP.md`](docs/port/ROADMAP.md) — milestones and current position
- [`TODO.md`](TODO.md) — central branch-aware checklist, next tasks and completion gates
- [`docs/port/VERIFICATION.md`](docs/port/VERIFICATION.md) — how parity with C is proven

## Development setup

The optional local setup includes `ngspice-rs` (development main with C
references), `ngspice-rs-diffsol-faer` (solver integration), `ngspice-rs-dist`
(Rust-only publication) and `ngspice_test` (C oracle). Inspect the chosen branch
and status; do not assume these checkouts are synchronized.

The development checkout provides `scripts/publish-rust-only.sh`, exporting
tracked Rust files without C sources/history. Its publisher maps `rust-port` to
public `main` and retains other named branches. Before a whole-tree export,
ensure the source includes all newer public-side changes; current GitHub main
already contains the solver merge. Focused documentation publication must
preserve that implementation and keep the distinct histories separate.

Nothing in the port depends on that setup: `cargo test` and `cargo xtask ci` run
in any checkout. C capture/check and opt-in live oracles need an upstream
`ngspice` binary, supplied through `NGSPICE_BIN` or `--ngspice`.
