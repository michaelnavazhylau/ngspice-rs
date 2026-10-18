# ngspice-rs

[![crates.io](https://img.shields.io/crates/v/ngspice-rs.svg)](https://crates.io/crates/ngspice-rs)
[![docs.rs](https://img.shields.io/docsrs/ngspice-rs)](https://docs.rs/ngspice-rs)
[![License: BSD-3-Clause](https://img.shields.io/crates/l/ngspice-rs.svg)](COPYING)

A from-scratch Rust implementation of [ngspice](https://ngspice.sourceforge.io/),
the SPICE circuit simulator.

The port reads ngspice decks, runs DC, AC and transient analyses on a bounded set
of linear and nonlinear devices, and writes ngspice-compatible rawfiles. Results
are checked against output captured from upstream ngspice. It is **not** yet a
drop-in replacement: coverage is deliberately bounded, and anything outside it
fails with an explicit error instead of a partial or approximate result.

```sh
cargo run -p ngspice-rs -- simulate --output rc.raw conformance/netlists/rc_transient.cir
```

## What works

| Area | Supported |
| --- | --- |
| Netlists | Scalar R/C/L/V/I, Y/TXL lines, linear E/F/G/H controlled sources, B behavioural sources and E/G/F/H `VALUE`/`TABLE`/`POLY` forms, K mutual inductance, `.model`, D/Q/M instances, S/W switches with `sw`/`csw` models, `.param` and `{expr}`/`'expr'` expressions, `.func` user functions, `.option` (common simulator options incl. `gmin`, `itl1`/`itl2`/`itl4`, `xmu`, `{expr}` values; documented no-ops) and `.global`, subcircuits and `X` instances, `.include`/`.lib`, numeric PULSE (with pulse count)/PWL (with `td=`/`r=`)/SIN/EXP/SFFM/AM sources, `.ic` |
| Devices | Linear R/C/L/V/I, V sources as RF ports (`portnum`/`z0`); linear E/F/G/H controlled sources (LAPLACE unported); B behavioural sources and the lowered E/G/F/H VALUE/TABLE/POLY forms (`ddt` and the statistical `agauss`/`gauss`/`aunif`/`unif`/`limit` unported); K mutual inductance (two or more inductors, literal or model-backed, in subcircuits, OP/AC/trap/Gear-2, coupled `ic=`/`uic` on the companion driver); model-backed passives (geometry, TC1/TC2, scale, multiplicity); diode (breakdown, sidewall junctions, recombination and tunnelling currents, temperature); Gummel-Poon BJT (base charge, leakage, series resistances, transit time, substrate junction, temperature); MOS levels 1, 2, 3, 6 and 9 (MOS1: Meyer gate charge, series resistance, junction geometry, process extraction and temperature); JFET levels 1 and 2 (level 2 is Parker-Skellern); MESFET level 1 (Statz), MESA levels 2-4 (`mesa1`/`mesa2`/`mesa3`; level 4's swept small-signal analyses are refused because C's own `mesaacl.c` reads fields `mesa3` never writes), HFET1 (level 5, `hfeta` with both gate models, the `a1`/`a2` corrections and the `cds` charge) and HFET2 (level 6, `hfeta2`); C defines no noise or distortion routine for this family, and its `.sens` tables are ported (see [SENSITIVITY.md](docs/port/SENSITIVITY.md)); S/W voltage- and current-controlled switches (hysteresis, ON/OFF, step control, C's AC state; companion `.tran` only); T lossless, O lossy (LTRA), U uniform-RC and Y single-lossy (TXL) lines (DC and - for T/O/U - exact AC, companion `.tran` with device breakpoints; U expands as `urcsetup.c` and a high-`R/L` Y as `inp2y.c` do; TXL supports DC/`.tf`/transient but refuses AC/SP rather than reuse C's time-domain load; see [TXL](docs/port/TXL.md)) |
| Analyses | `.op`; `.dc` over V/I sources, resistors, temperature and settable instance parameters (`@inst[param]`), including nested sweeps; small-signal `.ac`; `.noise` spectra and integrated totals; `.pz` poles and zeros at the operating point; `.tf` DC gain and input/output resistance; `.sp` S-parameters (S/Y/Z over RF ports, RF noise covariances and two-port noise parameters); `.disto` harmonic and intermodulation distortion (`distof1`/`distof2` inputs; D, Gummel-Poon Q and MOS1 nonlinearities); `.sens` DC and AC sensitivities by C's finite-difference perturbation (R/C/L/K/V/I/E/F/G/H, and in DC also B, S/W and D); `.tran` with adaptive trapezoidal / Gear-2 integration, `.ic` and `uic` (linear and nonlinear, including D/Q/M `off` and `ic=`); `.nodeset` as C's `MODEINITJCT`/`MODEINITFIX` hint; nonlinear operating points use ngspice's continuation (dynamic and stepped gmin, Gillespie and spice3 source stepping, `noopiter`) and its PN-junction/FET voltage limiting |
| Output | ASCII/binary rawfiles from the CLI (one plot per analysis for multi-analysis decks, in ngspice batch order), ASCII and binary rawfile read/write in the library, per-analysis `.save`/`.print` selection and bounded ASCII `.plot` rendering |
| Post-processing | A bounded `.measure` subset (`FIND … AT=`, `MIN`/`MAX`/`AVG`/`RMS`/`INTEG`, `TRIG … TARG …`) and `.four` |

Each area has documented limits; see [the feature guides](#documentation).
Not yet supported: BSIM3/BSIM4 (planned as M11, [BSIM.md](docs/port/BSIM.md)),
model-binning deck goldens, `.sens` of BJTs and MOSFETs, AC `.sens` of
nonlinear devices, the
remaining unported asks and levels of bounded devices (AC device current/power
asks and the charge-history asks `@z[...][qgs|cqgs|qgd|cqgd]` fail with
`NotYetPorted` naming the C ask routine); XSPICE code models,
ODSI/Verilog-A and CIDER; higher-index DAEs; and the interactive `.control`
interpreter. See [Status and roadmap](#status-and-roadmap).

## Status and roadmap

| Milestone | Scope | Status |
| --- | --- | --- |
| M1–M5 | Netlist front end, linear core, transient and index-one DAEs, diode/BJT/MOS1, CLI, subcircuits, `.measure`/`.four`, rawfiles | Done |
| M6 | Common decks: E/F/G/H, B sources, K coupling, S/W switches, SIN/EXP/SFFM/AM sources, `.func`, broader `.option`, multiple analyses per deck | Done |
| M7 | Diode physics, Gummel-Poon BJT, complete MOS1, ngspice convergence parity, nonlinear `.ic`/`uic` | Done |
| M8 | Additional analyses: `.noise`, `.tf`, `.sens`, `.pz`, `.disto`, `.sp`, Gear orders 3–6, wider `.dc` sweeps | Done within documented limits |
| M9 | Output and front end: `.plot`, binary CLI rawfiles, device currents, full `.measure`/`.four`, cards inside `.subckt` | Delivered with documented bounds; see TODO.md |
| M10 | Device library: JFET 1/2, MESFET 1, MESA 2/3/4, HFET1/2, MOSFET levels 2/3/6/9, T/O/U/Y lines, model binning resolver | Delivered in waves 0–3 and the spill slices H1a–H4 and TX1–TX4 within documented limits; BSIM3/4 spill to M11 |
| M11 | BSIM3v3.3 and BSIM4.7+, binning deck goldens | Planned: issue #90 moved off M10 so that M10 closes on the device library it defines. Plan in [BSIM.md](docs/port/BSIM.md) |

Each milestone is tracked as a
[GitHub milestone](https://github.com/michaelnavazhylau/ngspice-rs/milestones)
with one issue per feature. XSPICE, OSDI/Verilog-A, CIDER and the `.control`
interpreter are out of the initial scope and tracked as design follow-ups.
Detailed progress is in [TODO.md](TODO.md); [ROADMAP.md](docs/port/ROADMAP.md)
records the design of the earlier milestones.

## Installation

`ngspice-rs` is published on [crates.io](https://crates.io/crates/ngspice-rs) and
needs Rust **1.89** or newer (edition 2024). No C toolchain or ngspice binary is
required.

The `spice-rs` command-line tool can be installed as a prebuilt binary with
[cargo-binstall](https://github.com/cargo-bins/cargo-binstall), or compiled
from source with `cargo install`:

```sh
cargo binstall ngspice-rs    # prebuilt binary, no compilation
cargo install ngspice-rs     # build from crates.io source
spice-rs simulate --output out.raw deck.cir
```

Prebuilt binaries are attached to each
[GitHub release](https://github.com/michaelnavazhylau/ngspice-rs/releases) for
Linux (x86_64 and aarch64, static musl), macOS (Intel and Apple silicon) and
Windows (x86_64); they can also be downloaded directly.

To use the simulator as a library:

```sh
cargo add ngspice-rs
```

The library is organised as the modules `primitives`, `netlist`, `maths`,
`devices`, `analysis` and `cli`; the API reference is on
[docs.rs](https://docs.rs/ngspice-rs). The 0.x API is not yet stable.

## Building from source

`rust-toolchain.toml` selects stable with clippy and rustfmt. No C toolchain or
ngspice binary is needed to build or test.

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
(D/Q/M/J/Z/O/U, S/W and X: built from the deck, through `.model` or `.subckt`,
for the stated subset) or `pending` (`NotYetPorted`); `analyses` lists the
analysis drivers and `.four` as a post-processor of `.tran`. Both are derived
from the code and pinned by tests (#117).

`simulate` runs every analysis card of a deck in ngspice batch order (`.ac`,
`.dc`, `.op`, `.tran`, `.pz`, `.tf`, `.disto`, `.noise`, `.sens`, `.sp`, with
`.four` post-processing the `.tran` plot) and writes one plot per analysis;
a failure in any analysis publishes nothing. Exit status
is `0` on success, `1` for a bad command line, `2` for a bad deck or failed run,
and `3` when the deck needs something the port does not support yet. Details are
in [CLI.md](docs/port/CLI.md).

The engine can also be used as a library: parse a deck, build a `Circuit`
and run `analysis::runner(kind)` (see
[NATIVE_SOLVER.md](docs/port/NATIVE_SOLVER.md)).

## Transient solvers

An ordinary `.tran` uses the adaptive trapezoidal / Gear-2 companion driver with
truncation-error control and breakpoint handling, matching ngspice's approach
([TRANSIENT.md](docs/port/TRANSIENT.md)); it is the only transient backend.
Linear systems are factored by a Rust port of ngspice's own sparse solver,
SPARSE 1.3, with C's pivoting and error policy
([NATIVE_SOLVER.md](docs/port/NATIVE_SOLVER.md)). The earlier diffsol BDF
backend and faer LU were removed in M10 wave 0.5; `backend=diffsol` is an
explicit error.

## Verification

The C implementation is used only as an out-of-process oracle; there is no FFI.
`conformance/netlists/` holds fixture decks (linear, diode, BJT, MOSFET,
subcircuit and transient cases), and `conformance/golden/` holds the rawfiles
upstream ngspice produced for them, committed so tests run without C.

```sh
cargo xtask golden verify                            # Rust engine vs committed C output
NGSPICE_BIN=/path/to/ngspice cargo xtask golden check  # C output still reproduces
```

`golden verify` verifies every committed golden fixture except
`multi_analysis_pz`, whose pole search depends on the C build's fused
multiply-adds (see [VERIFICATION.md](docs/port/VERIFICATION.md)).
[VERIFICATION.md](docs/port/VERIFICATION.md) describes the harness, tolerances
and its limits.

## Repository layout

```
src/primitives       numbers, units, nodes, errors, analysis taxonomy
src/netlist          deck loading, tokenizer, winnow parser, AST, parameter evaluation
src/maths            dense/sparse/complex LU (SPARSE 1.3 port), trap/Gear integration
src/devices          device models, MNA stamping, device registry
src/analysis         analysis drivers, plots, measurements, rawfile I/O
src/cli              CLI argument parsing, dispatch and reporting
src/bin/spice-rs.rs  the `spice-rs` binary
tests/               integration tests, one target per file
examples/            library-level examples (equilibration benchmark)
xtask/               golden capture/verification, snapshots, CI (unpublished
                     workspace member, not part of the published package)
conformance/         fixture decks and captured ngspice output
docs/port/           architecture, C-to-Rust mapping, roadmap and feature guides
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
  [TRANSMISSION_LINES.md](docs/port/TRANSMISSION_LINES.md),
  [BEHAVIOURAL_SOURCES.md](docs/port/BEHAVIOURAL_SOURCES.md)
- Analyses: [DC_SWEEPS.md](docs/port/DC_SWEEPS.md),
  [DC_CONTINUATION.md](docs/port/DC_CONTINUATION.md),
  [TRANSIENT.md](docs/port/TRANSIENT.md),
  [TRANSFER_FUNCTION.md](docs/port/TRANSFER_FUNCTION.md),
  [SPARAM.md](docs/port/SPARAM.md),
  [SENSITIVITY.md](docs/port/SENSITIVITY.md)
- Output: [CLI.md](docs/port/CLI.md), [RAWFILES.md](docs/port/RAWFILES.md),
  [OUTPUT_SELECTION.md](docs/port/OUTPUT_SELECTION.md),
  [MEASURE.md](docs/port/MEASURE.md), [FOURIER.md](docs/port/FOURIER.md),
  [CONTROL_SCRIPTS_ADR.md](docs/port/CONTROL_SCRIPTS_ADR.md) (the bounded
  `.control` settings block)
- Numerics: [NATIVE_SOLVER.md](docs/port/NATIVE_SOLVER.md),
  [EQUILIBRATION.md](docs/port/EQUILIBRATION.md),
  [SPARSE_RANK_DIAGNOSTICS.md](docs/port/SPARSE_RANK_DIAGNOSTICS.md),
  [HIGHER_INDEX_DAE_ADR.md](docs/port/HIGHER_INDEX_DAE_ADR.md)
- Releases: [RELEASING.md](docs/port/RELEASING.md) (crates.io publish workflow)

## License

Modified BSD, the same license as ngspice. This is a **derivative work**: the Rust
code reimplements behaviour defined by ngspice's C sources. [COPYING](COPYING) is
reproduced verbatim from upstream and covers the few contributions with different
terms; [AUTHORS](AUTHORS) is upstream's attribution. See [NOTICE](NOTICE). The
Rust dependencies (winnow, petgraph) are permissively licensed, and no LGPL KLU
code is used; the SPARSE 1.3 port carries its Berkeley notice.
