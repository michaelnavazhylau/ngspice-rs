# ngspice-rs

A from-scratch Rust implementation of [ngspice](https://ngspice.sourceforge.io/),
the SPICE circuit simulator.

The port reads ngspice decks, runs DC, AC and transient analyses on a bounded set
of linear and nonlinear devices, and writes ngspice-compatible rawfiles. Results
are checked against output captured from upstream ngspice. It is **not** yet a
drop-in replacement: coverage is deliberately bounded, and anything outside it
fails with an explicit error instead of a partial or approximate result.

```sh
cargo run -p spice-cli -- simulate --output rc.raw conformance/netlists/rc_transient.cir
```

## What works

| Area | Supported |
| --- | --- |
| Netlists | Scalar R/C/L/V/I, linear E/F/G/H controlled sources, B behavioural sources and E/G/F/H `VALUE`/`TABLE`/`POLY` forms, K mutual inductance, `.model`, D/Q/M instances, S/W switches with `sw`/`csw` models, `.param` and `{expr}`/`'expr'` expressions, `.func` user functions, `.option` (common simulator options incl. `gmin`, `itl1`/`itl2`/`itl4`, `xmu`, `{expr}` values; documented no-ops) and `.global`, subcircuits and `X` instances, `.include`/`.lib`, numeric PULSE (with pulse count)/PWL (with `td=`/`r=`)/SIN/EXP/SFFM/AM sources, `.ic` |
| Devices | Linear R/C/L/V/I; linear E/F/G/H controlled sources (LAPLACE unported); B behavioural sources and the lowered E/G/F/H VALUE/TABLE/POLY forms (`ddt` and the statistical `agauss`/`gauss`/`aunif`/`unif`/`limit` unported); K mutual inductance (two or more inductors, literal or model-backed, in subcircuits, OP/AC/trap/Gear-2/BDF, coupled `ic=`/`uic` on the companion driver); model-backed passives (geometry, TC1/TC2, scale, multiplicity); diode, Gummel-Poon BJT and MOS1 (level 1); S/W voltage- and current-controlled switches (hysteresis, ON/OFF, step control, C's AC state; companion `.tran` only) |
| Analyses | `.op`; `.dc` over V/I sources, resistors and temperature, including nested sweeps; small-signal `.ac`; `.tran` with adaptive trapezoidal / Gear-2 integration, `.ic` and `uic` |
| Output | ASCII rawfiles from the CLI (one plot per analysis for multi-analysis decks, in ngspice batch order), ASCII and binary rawfile read/write in the library, per-analysis `.save`/`.print` selection |
| Post-processing | A bounded `.measure` subset (`FIND … AT=`, `MIN`/`MAX`/`AVG`/`RMS`/`INTEG`, `TRIG … TARG …`) and `.four` |

Each area has documented limits; see [the feature guides](#documentation).
Notable gaps include advanced BSIM models, XSPICE, OSDI/Verilog-A, CIDER, `.plot`,
nonlinear transient initialization, higher-index DAEs and the interactive
interpreter. Remaining work is tracked in [TODO.md](TODO.md);
milestones are in [ROADMAP.md](docs/port/ROADMAP.md).

## Getting started

Rust **1.89** or newer is required (edition 2024); `rust-toolchain.toml` selects
stable with clippy and rustfmt. No C toolchain or ngspice binary is needed to
build or test.

```sh
cargo build --release                    # builds the `spice-rs` binary
cargo test --workspace --locked          # full test suite
cargo xtask ci                           # fmt --check, clippy -D warnings, tests
```

For offline builds, fetch the locked dependencies once:

```sh
cargo fetch --locked
cargo test --workspace --locked --offline
```

## Command line

```sh
spice-rs simulate --output out.raw deck.cir   # run the deck's analyses, write a rawfile
spice-rs parse deck.cir                       # parse and report unported constructs
spice-rs cards deck.cir                       # classify every card
spice-rs tokens deck.cir                      # dump the token stream
spice-rs devices                             # list supported device designators
spice-rs analyses                            # list analyses and their status
```

`devices` marks each designator `ported` (built from its card), `bounded`
(D/Q/M, S/W and X: built from the deck, through `.model` or `.subckt`, for the
stated subset) or `pending` (`NotYetPorted`); `analyses` lists the four drivers
and `.four` as a post-processor of `.tran`. Both are derived from the code and
pinned by tests (#117).

`simulate` runs every `.op`, `.dc`, `.ac` and `.tran` card of a deck in ngspice
batch order (`.ac`, `.dc`, `.op`, `.tran`) and writes one plot per analysis;
a failure in any analysis publishes nothing. Exit status
is `0` on success, `1` for a bad command line, `2` for a bad deck or failed run,
and `3` when the deck needs something the port does not support yet. Details are
in [CLI.md](docs/port/CLI.md).

The engine can also be used as a library; `crates/spice-analysis/examples/rc_diffsol.rs`
shows the production simulation API.

## Transient solvers

An ordinary `.tran` uses the adaptive trapezoidal / Gear-2 companion driver with
truncation-error control and breakpoint handling, matching ngspice's approach
([TRANSIENT.md](docs/port/TRANSIENT.md)). Adding `backend=diffsol method=bdf` to
the `.tran` card selects an alternative adaptive BDF integrator from
[diffsol](https://github.com/martinjrobins/diffsol); it supports linear index-one
DAEs only and is not equivalent to ngspice's integration methods. Linear systems
are factored with [faer](https://github.com/sarah-quinones/faer-rs).

## Verification

The C implementation is used only as an out-of-process oracle; there is no FFI.
`conformance/netlists/` holds fixture decks (linear, diode, BJT, MOSFET,
subcircuit and transient cases), and `conformance/golden/` holds the rawfiles
upstream ngspice produced for them, committed so tests run without C.

```sh
cargo xtask golden verify                            # Rust engine vs committed C output
NGSPICE_BIN=/path/to/ngspice cargo xtask golden check  # C output still reproduces
```

`golden verify` currently verifies all 69 golden fixtures (two of them
four-plot multi-analysis decks) with no exclusions.
[VERIFICATION.md](docs/port/VERIFICATION.md) describes the harness, tolerances
and its limits.

## Repository layout

```
crates/spice-core       numbers, units, nodes, errors, analysis taxonomy
crates/spice-netlist    deck loading, tokenizer, winnow parser, AST, parameter evaluation
crates/spice-maths      dense/sparse/complex LU (faer), BDF integration (diffsol)
crates/spice-devices    device models, MNA stamping, device registry
crates/spice-analysis   analysis drivers, plots, measurements, rawfile I/O
crates/spice-cli        the `spice-rs` binary
xtask                   golden capture/verification, snapshots, CI
conformance/            fixture decks and captured ngspice output
docs/port/              architecture, C-to-Rust mapping, roadmap and feature guides
```

## Documentation

- Design: [ARCHITECTURE.md](docs/port/ARCHITECTURE.md),
  [MAPPING.md](docs/port/MAPPING.md) (C sources to Rust modules),
  [ROADMAP.md](docs/port/ROADMAP.md), [TODO.md](TODO.md), [RUST_PORT.md](RUST_PORT.md)
- Front end: [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md),
  [FRONTEND_VALUES.md](docs/port/FRONTEND_VALUES.md),
  [PARAM_EXPRESSIONS.md](docs/port/PARAM_EXPRESSIONS.md),
  [SUBCIRCUITS.md](docs/port/SUBCIRCUITS.md)
- Devices: [MODEL_SCHEMAS.md](docs/port/MODEL_SCHEMAS.md),
  [PASSIVE_MODELS.md](docs/port/PASSIVE_MODELS.md),
  [M4_NONLINEAR.md](docs/port/M4_NONLINEAR.md),
  [CONTROLLED_SOURCES.md](docs/port/CONTROLLED_SOURCES.md),
  [MUTUAL_INDUCTANCE.md](docs/port/MUTUAL_INDUCTANCE.md),
  [SWITCHES.md](docs/port/SWITCHES.md),
  [BEHAVIOURAL_SOURCES.md](docs/port/BEHAVIOURAL_SOURCES.md)
- Analyses: [DC_SWEEPS.md](docs/port/DC_SWEEPS.md),
  [DC_CONTINUATION.md](docs/port/DC_CONTINUATION.md),
  [TRANSIENT.md](docs/port/TRANSIENT.md)
- Output: [CLI.md](docs/port/CLI.md), [RAWFILES.md](docs/port/RAWFILES.md),
  [OUTPUT_SELECTION.md](docs/port/OUTPUT_SELECTION.md),
  [MEASURE.md](docs/port/MEASURE.md), [FOURIER.md](docs/port/FOURIER.md)
- Numerics: [DIFFSOL_FAER_IMPLEMENTATION.md](docs/port/DIFFSOL_FAER_IMPLEMENTATION.md),
  [EQUILIBRATION.md](docs/port/EQUILIBRATION.md),
  [SPARSE_RANK_DIAGNOSTICS.md](docs/port/SPARSE_RANK_DIAGNOSTICS.md),
  [HIGHER_INDEX_DAE_ADR.md](docs/port/HIGHER_INDEX_DAE_ADR.md)

## License

Modified BSD, the same license as ngspice. This is a **derivative work**: the Rust
code reimplements behaviour defined by ngspice's C sources. [COPYING](COPYING) is
reproduced verbatim from upstream and covers the few contributions with different
terms; [AUTHORS](AUTHORS) is upstream's attribution. See [NOTICE](NOTICE). The
Rust dependencies (winnow, petgraph, faer, diffsol) are permissively licensed, and
no LGPL KLU code is used.
