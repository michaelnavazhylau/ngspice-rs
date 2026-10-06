# ngspice-rs

A from-scratch Rust implementation of [ngspice](https://ngspice.sourceforge.io/),
the SPICE circuit simulator.

> **Status: M1 in progress.** The parser now builds semantic netlists for scalar
> R/C/L devices, DC/AC V/I sources, scalar model cards and bounded D/Q/M
> instances. Six fixture decks parse successfully. Model-backed passives,
> waveforms, subcircuits and parameter expressions remain unported.
> **There is no simulation engine yet.** Gaps return `SpiceError::NotYetPorted`
> naming the upstream C file, with CLI exit status 3 rather than partial success.

See [RUST_PORT.md](RUST_PORT.md#status) for the per-area status table,
[docs/port/ROADMAP.md](docs/port/ROADMAP.md) for the milestones,
[docs/port/TODO.md](docs/port/TODO.md) for the next tasks, and
[docs/port/ARCHITECTURE.md](docs/port/ARCHITECTURE.md) for the crate layout.

## Quick start

```sh
cargo test                              # the whole suite, no C toolchain needed
cargo xtask ci                          # fmt --check, clippy -D warnings, test
cargo run -p spice-cli -- parse conformance/netlists/rc_divider.cir
cargo xtask golden list                 # what the captured comparison data holds
```

## Parsing backend (`new-parsing`)

Semantic parsing uses **winnow 1.0.4** over borrowed, positioned tokens. Card
alternatives, terminals, scalar assignments and DC/AC source forms use parser
combinators; the existing deck loader and tokenizer are unchanged. This branch
preserves M1a's AST and CLI exit contract. M1b adds scalar D/BJT/MOS/R/C/L
model cards, two-terminal diodes, three/four-terminal BJTs and four-terminal MOS
instances. Q/M require in-deck model declarations (forward references work)
for terminal disambiguation. Model type/backend and parameter validity remain
elaboration work; parsing is not simulation.

Winnow is the first external dependency, used only by `spice-netlist`, with its
`std` and `parser` features. A fresh checkout needs a registry download; cache
the locked dependencies once for subsequent offline builds:

```sh
cargo fetch --locked
cargo test --workspace --locked --offline
```

## Layout

```
crates/spice-core       numbers, units, nodes, errors, analysis taxonomy
crates/spice-netlist    deck loading, tokenizer, card classification, AST
crates/spice-maths      dense and sparse storage, integration types
crates/spice-devices    Device trait, MNA stamping, device registry
crates/spice-analysis   analysis dispatch, plots, ASCII rawfile read and write
crates/spice-cli        the `spice-rs` binary
xtask                   golden capture and drift checks, CI
conformance/            fixture decks and the rawfiles captured from ngspice
docs/port/              architecture, C-to-Rust mapping, roadmap, verification
```

## Verification

There is no FFI: the C implementation is used only as an oracle, out of process.
`conformance/netlists/` holds eight decks that exercise an operating point, an AC
sweep, a transient run, a DC sweep, a diode, a BJT, a MOSFET and a subcircuit;
`conformance/golden/` holds the ASCII rawfile that upstream `ngspice-47+`
produced for each of them, committed so the tests run without a C toolchain.

To check that the committed data still reproduces, point the harness at a built
upstream `ngspice`:

```sh
NGSPICE_BIN=/path/to/ngspice cargo xtask golden check
cargo xtask golden check --ngspice /path/to/ngspice   # equivalent
```

[docs/port/VERIFICATION.md](docs/port/VERIFICATION.md) describes the harness and
lists the behaviours it discovered that the port has to reproduce.

## License

Modified BSD, the same license as ngspice, because this is a **derivative work**:
the Rust code reimplements behaviour defined by ngspice's C sources.
[COPYING](COPYING) is reproduced verbatim from upstream and includes the few
contributions that carry different terms; [AUTHORS](AUTHORS) is upstream's
attribution. See [NOTICE](NOTICE).
