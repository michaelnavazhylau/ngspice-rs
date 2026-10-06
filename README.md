# ngspice-rs

A from-scratch Rust implementation of [ngspice](https://ngspice.sourceforge.io/),
the SPICE circuit simulator.

> **Status: M1 in progress; a bounded linear simulation engine is implemented.**
> Scalar R/C/L/V/I devices support `.op`, single-source `.dc`, complex `.ac`
> and explicitly selected diffsol BDF transient analysis. Scalar models and
> bounded D/Q/M syntax parse, but nonlinear D/Q/M simulation is not implemented.
> Six of eight fixture decks parse; full SPICE parity is not claimed.

[TODO.md](TODO.md) is the central implementation checklist, including branch-aware
status, remaining work and completion gates. See
[docs/port/ROADMAP.md](docs/port/ROADMAP.md) for milestones,
[docs/port/ARCHITECTURE.md](docs/port/ARCHITECTURE.md) for crate boundaries, and
[RUST_PORT.md](RUST_PORT.md#status) for per-area details.

## Current capabilities and remaining work

GitHub `main` includes the solver implementation (`31e245f`, merged by `b467ca0`).
The local C-reference development mainline at `d3c8cccf4` predates that work;
its history differs from this Rust-only repository. Do not overwrite newer public
code with a whole-tree export from an older development checkout.

| Capability | GitHub main |
| --- | --- |
| Scalar/model/D/Q/M parsing and petgraph topology | Implemented, bounded syntax |
| Scalar R/C/L/V/I simulation and real/complex LU | Implemented using faer |
| Linear `.op`, single-source `.dc`, complex `.ac` | Implemented |
| Transient | Explicit diffsol adaptive BDF, restricted DAE structure |
| Nonlinear D/Q/M equations | Not implemented |
| CLI simulation command | Not implemented; APIs/examples only |

Transient requires explicit `backend=diffsol method=bdf`; it is
**not ngspice trapezoidal or fixed Gear-2 and does not complete M3**. It currently
requires diagonal mass structure with a nonsingular algebraic block. Floating/
coupled capacitor DAEs, higher-index constraints, nonlinear charge and `.ic`/`uic`
remain unsupported. Step/Pwl waveforms exist through the device API only;
waveform netlist syntax still needs implementation.

Remaining work is tracked only in [TODO.md](TODO.md):

1. **Verification:** extend production conformance coverage and add Rust-engine
   `golden verify` tooling; preserve the implemented solver's correctness gates.
2. **Front end (M1):** waveform syntax, flags/vector ICs, model-backed passives,
   subcircuits/includes, parameters/options/globals, serialization and snapshots.
3. **Model elaboration:** resolution, family compatibility, typed defaults,
   level selection and parameter validation.
4. **Transient (M3):** trap/Gear-2 companions, initialization, parsed waveform
   evaluation, broader DAEs and C parity.
5. **Nonlinear devices (M4):** diode/BJT/MOS1 equations, Newton/limiting/stepping,
   nonlinear DC/AC/transient and conformance fixtures.
6. **Usability (M5):** CLI simulation, subcircuit instantiation/scoping,
   measurements/output selection and binary rawfiles.

Advanced BSIM models, XSPICE, OSDI/Verilog-A, CIDER, Tcl, full numparam
compatibility and the interactive interpreter are outside the initial scope.
Unsupported cases must fail explicitly; CLI `NotYetPorted` errors exit 3 rather
than reporting partial success.

## Quick start

```sh
cargo test                              # the whole suite, no C toolchain needed
cargo xtask ci                          # fmt --check, clippy -D warnings, test
cargo run -p spice-cli -- parse conformance/netlists/rc_divider.cir
cargo xtask golden list                 # what the captured comparison data holds
cargo run -p spice-analysis --example rc_diffsol --locked # production simulation API
```

## Continuous integration

Separate GitHub Actions workflows run on every push and pull request, and can
also be started manually. Both run on Ubuntu with dependency caching; the test
workflow covers stable Rust and the declared MSRV, Rust 1.89:

- **Build**: `cargo build --workspace --locked --release`.
- **Tests**: `cargo test --workspace --locked` and all-target Clippy with warnings
  denied, including doctests and committed conformance fixtures. Opt-in live C
  oracle tests remain ignored; no ngspice binary or C toolchain is required.

## Parsing backend (`new-parsing`)

Semantic parsing uses **winnow 1.0.4** over borrowed, positioned tokens. Card
alternatives, terminals, scalar assignments and DC/AC source forms use parser
combinators; the existing deck loader and tokenizer are unchanged. The parser
preserves M1a's AST and CLI exit contract. M1b adds scalar D/BJT/MOS/R/C/L
model cards, two-terminal diodes, three/four-terminal BJTs and four-terminal MOS
instances. Q/M require in-deck model declarations (forward references work)
for terminal disambiguation. Model type/backend and parameter validity remain
elaboration work; parsing is not simulation.

Winnow was the first external dependency and is used only by `spice-netlist`,
with its `std` and `parser` features. Petgraph is also used in production topology
APIs; faer supplies real/complex LU and diffsol supplies the bounded BDF backend.
SuiteSparse/SUNDIALS features remain disabled. A fresh checkout needs registry
downloads; cache the locked dependencies once for subsequent offline builds:

```sh
cargo fetch --locked
cargo test --workspace --locked --offline
```

## Layout

```
crates/spice-core       numbers, units, nodes, errors, analysis taxonomy
crates/spice-netlist    deck loading, tokenizer, card classification, AST
crates/spice-maths      dense/sparse/complex storage, LU, bounded BDF integration
crates/spice-devices    Device trait, MNA stamping, device registry
crates/spice-analysis   analysis dispatch, plots, ASCII rawfile read and write
crates/spice-cli        the `spice-rs` binary
xtask                   golden capture and drift checks, CI
conformance/            fixture decks and the rawfiles captured from ngspice
TODO.md                 central branch-aware implementation checklist
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
its limits. `golden check` checks reproducibility of C output; it does not run
the Rust simulation engine. Production DC/AC golden comparisons and analytic/
live-C transient tests are documented in
[DIFFSOL_FAER_IMPLEMENTATION.md](docs/port/DIFFSOL_FAER_IMPLEMENTATION.md), including
recorded validation and justified tolerances. No new test run is implied by this
status summary.

## License

Modified BSD, the same license as ngspice, because this is a **derivative work**:
the Rust code reimplements behaviour defined by ngspice's C sources.
[COPYING](COPYING) is reproduced verbatim from upstream and includes the few
contributions that carry different terms; [AUTHORS](AUTHORS) is upstream's
attribution. See [NOTICE](NOTICE).
