# ngspice-rs

A from-scratch Rust implementation of [ngspice](https://ngspice.sourceforge.io/),
the SPICE circuit simulator.

> **Status: scaffold.** The workspace compiles, the test suite passes, and the
> conformance harness can capture comparison data from the reference C binary —
> but the netlist parser, the device models and the analyses are **not
> implemented**. Every gap returns `SpiceError::NotYetPorted`, naming the
> upstream C file it has to be ported from, and `spice-rs parse` exits with
> status 3 rather than pretending to succeed.

See [RUST_PORT.md](RUST_PORT.md#status) for the per-area status table,
[docs/port/ROADMAP.md](docs/port/ROADMAP.md) for the milestones, and
[docs/port/ARCHITECTURE.md](docs/port/ARCHITECTURE.md) for the crate layout.

## Quick start

```sh
cargo test                              # the whole suite, no C toolchain needed
cargo xtask ci                          # fmt --check, clippy -D warnings, test
cargo run -p spice-cli -- conformance/netlists/rc_divider.cir
cargo xtask golden list                 # what the captured comparison data holds
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
