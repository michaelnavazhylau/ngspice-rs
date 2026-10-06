# RUST_PORT.md — a Rust port of ngspice

A Cargo workspace for a from-scratch Rust implementation of ngspice. Development
happens in an ngspice worktree (`new-parsing`, branched from `rust-port`);
the public standalone repository
contains only the Rust port and its conformance data.

**Nothing in the C sources is modified or replaced.** ngspice is the reference
implementation, inspected during porting and queried out of process. Verification
is incremental; see [`docs/port/VERIFICATION.md`](docs/port/VERIFICATION.md) for
what is proven and what is still missing.

The Rust port is a **derivative work of ngspice** and is distributed under the
same Modified BSD license (see [`COPYING`](COPYING)).

## Status

**M1 in progress; M1a and basic model/D/Q/M syntax are implemented.**
The workspace compiles,
the test suite passes, and the verification harness captures golden data from
the C binary. The parser builds ASTs for scalar R/C/L, DC/AC V/I, scalar
D/BJT/MOS/R/C/L model cards and bounded D/Q/M instances. Q accepts three ports
plus optional substrate; M accepts drain/gate/source/bulk and scalar geometry.
Q/M require a declaration in the same deck before `.end` (forward definitions
work); the declaration index disambiguates ports, not model backends.
Model-backed passives, waveforms, subcircuits and expressions remain unported.
Model types, keyword validity and defaults/selector rules remain elaboration
work; AST success does not imply backend availability.
Device implementations, solvers and analysis drivers are still stubbed — every
unimplemented entry point returns
[`SpiceError::NotYetPorted`](crates/spice-core/src/error.rs) naming its C reference.

What already works for real:

| Area | Crate | Notes |
| --- | --- | --- |
| SPICE numeric literals and scale factors | `spice-core` | `INPevaluate()` table from `src/spicelib/parser/inpeval.c` |
| Node table, ground aliasing | `spice-core` | `inp_fix_gnd_name()` from `src/frontend/inpcom.c` |
| Deck loading: title, continuation, comments | `spice-netlist` | `inp_stripcomments_line()`, `inp_readall()` |
| Card tokenizer and `.command` classification | `spice-netlist` | `inppas2.c` / `inp2dot.c` dispatch |
| Winnow semantic parser | `spice-netlist` | borrowed token-stream combinators; scalar R/C/L, DC/AC V/I, scalar models, bounded D/Q/M, opaque analyses; six fixture decks parse |
| Opt-in live parser oracle | `spice-netlist` tests | compares scalar AST parameters and Q/M terminal order with live C queries |
| MNA matrix / triplet storage (no solver) | `spice-maths` | solver itself is stubbed |
| Petgraph topology APIs | `spice-devices`, `spice-maths` | circuit incidence/per-port edges and assembled matrix-row coupling; no DC-path/solvability claim |
| ASCII rawfile read *and* write | `spice-analysis` | `src/frontend/rawfile.c` format, byte-for-byte layout |
| Conformance fixtures and goldens | `conformance/`, `xtask` | 8 decks, captured from `ngspice-47+` |
| Golden-data capture and drift check | `xtask` | drives the C `ngspice` binary |

## Quick start

```sh
cargo build                          # build the workspace
cargo test                           # run the test suite, including conformance tests
cargo xtask help                     # automation commands
cargo xtask golden list              # what the captured goldens contain
cargo xtask golden check             # re-run C ngspice, diff against the goldens
cargo xtask ci                       # fmt --check + clippy -D warnings + test
```

`cargo test` needs Rust and the locked registry dependencies; the goldens are
committed data, so no C toolchain is required. Run `cargo fetch --locked` once
before `cargo test --workspace --locked --offline` for offline use.
`cargo xtask golden capture` needs a built C `ngspice` binary. Point at one with
`--ngspice <path>` or `NGSPICE_BIN=<path>`; otherwise it looks for
`build/src/ngspice` — which only exists if you are working inside an ngspice
checkout that has been built — and then for `ngspice` on `PATH`.

The CLI is `spice-rs`. It loads and tokenizes decks, classifies cards, and can
build semantic netlists for supported syntax. `spice-rs parse` succeeds on
`rc_divider`, `rc_lowpass_ac`, `rlc_series`, `diode_dc`, `bjt_ce` and
`mos_inverter`; `rc_transient` and `subckt_divider` still exit with status 3 at
an unported construct. **It does not simulate yet.**

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
- [`docs/port/TODO.md`](docs/port/TODO.md) — concrete next tasks and completion gates
- [`docs/port/VERIFICATION.md`](docs/port/VERIFICATION.md) — how parity with C is proven

## Development setup

The port is developed inside a git worktree of an ngspice checkout (`ngspice-rs`,
branch `new-parsing`, based on `rust-port`), so the C reference tree sits next
to the Rust code for
reading and for capturing comparison data. That worktree is the source of truth;
the standalone public repository is generated from it by
`scripts/publish-rust-only.sh` and holds the port alone, with no C sources and no
ngspice commit history. The publisher maps `rust-port` to public `main` and
preserves feature-branch names (`new-parsing` → `new-parsing`). The target must
be clean and checked out on the matching public branch before publishing.

Nothing in the port depends on that setup: `cargo test` and `cargo xtask ci` run
in any checkout, and only `cargo xtask golden capture` needs an upstream
`ngspice` binary, supplied through `NGSPICE_BIN` or `--ngspice`.
