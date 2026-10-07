# ngspice-rs

A from-scratch Rust implementation of [ngspice](https://ngspice.sourceforge.io/),
the SPICE circuit simulator.

> **Status: M1 front-end gate closed; bounded linear and M4 nonlinear subsets are implemented.**
> The branch-local nonlinear support/gate and deliberate physics limits are in
> [M4_NONLINEAR.md](docs/port/M4_NONLINEAR.md); no full SPICE parity is claimed.
> Scalar R/C/L/V/I devices support `.op`, single-source `.dc`, complex `.ac`
> and transient analysis (adaptive trap/Gear-2 companion driver; explicitly selected diffsol BDF). Scalar models and
> bounded D/Q/M and model-backed passive syntax parse. Top-level model resolution
> and bounded diode input schemas exist. This checkout also simulates bounded
> model-backed R/C/L plus bounded diode, Ebers-Moll BJT and MOS1 DC/AC/charge-companion transient.
> Numeric PULSE/PWL and bounded flags/IC vectors parse (syntax only).
> All eight original fixture decks and the new M4 charge decks parse; unsupported physics is rejected.

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

Passive syntax (#11) and initial model infrastructure (#17) are merged in
PR #51 (`cdc078c`). This checkout implements bounded passive elaboration (#19),
pending merge; see [PASSIVE_MODELS.md](docs/port/PASSIVE_MODELS.md) for its support
table, formulas, temperatures and deliberately rejected forms.

| Capability | Current checkout |
| --- | --- |
| Scalar/model/D/Q/M/passive-model parsing and petgraph topology | Implemented, bounded syntax |
| Scoped subcircuits/X and source-relative includes/libraries | Ordered scoped cards, bounded resolution/provenance; 8/8 fixture parses, no flattening |
| Model resolver and scalar schemas | Top-level families/levels/defaults; bounded passive and D/Q/M model-aware factories |
| Model-backed passives | Bounded R sheet/C area-perimeter geometry, L model value, TC1/TC2, scale and multiplicity |
| Scalar R/C/L/V/I simulation and real/complex LU | Implemented using faer |
| `.op`, typed/nested source/temperature `.dc`, bias-linearized `.ac` | Linear and bounded nonlinear devices |
| Transient | Ordinary `.tran`: adaptive trap/Gear-2 with bounded nonlinear charge; explicit diffsol BDF remains linear-only |
| Nonlinear D/Q/M equations | Bounded diode / Ebers-Moll BJT / MOS1; see M4 support table and explicit exclusions |
| CLI simulation command | Not implemented; APIs/examples only |

An ordinary `.tran` runs the adaptive trapezoidal / Gear-2 companion driver
([TRANSIENT.md](docs/port/TRANSIENT.md)); explicit `backend=diffsol method=bdf`
selects the adaptive BDF backend, which is
**not ngspice trapezoidal or fixed Gear-2**. M3's RC/RL/RLC, PWL, floating/coupled
capacitor and AC exit gates against C goldens are closed for the linear decks
(`cargo xtask golden verify`, `crates/spice-analysis/tests/m3_gate.rs`); M3 is not
complete: higher-index constraints (#29) and subcircuits remain unsupported.
M4 adds bounded nonlinear charge to the companion path, not general MNA DAE support. The BDF backend currently
accepts index-one DAEs, including floating/coupled capacitor networks; higher-index
constraints, nonlinear charge and `.ic`/`uic` remain unsupported. Numeric PULSE/PWL V/I setters elaborate into
Pulse/Pwl forcing (#9), with C's PULSE defaults taken from the `.tran` step/stop
time and explicit left/right limits at jumps; Step is device-API only. See
[FRONTEND_VALUES.md](docs/port/FRONTEND_VALUES.md).

Remaining work is tracked only in [TODO.md](TODO.md):

1. **Verification:** extend the bounded Rust-engine `golden verify` registry
   as support lands; preserve the implemented solver's correctness gates.
2. **Front end (M1):** `.param`/expression syntax (#14, [PARAM_EXPRESSIONS.md](docs/port/PARAM_EXPRESSIONS.md)), `.option`/`.global` parsing with a bounded `RunConfig` (#16) and top-level `.param` evaluation (#15, `spice_netlist::eval`/`elaborate`) are done; subcircuit parameters remain.
   Normalized deck serialization exists (#20, `spice_netlist::write_netlist`); token/AST snapshots exist (#21, `cargo xtask snapshots`); the eight-fixture M1 front-end round-trip gate is closed (#22, `crates/spice-netlist/tests/m1_gate.rs`); subcircuit flattening (#18, M5), subcircuit-scoped params and extended passive forms remain.
   Scoped/source syntax (#12/#13) is documented in [FRONTEND_STRUCTURE.md](docs/port/FRONTEND_STRUCTURE.md).
3. **Model elaboration:** extended passive forms, additional device schemas
   and scoped resolution; bounded passive geometry/temperature arithmetic exists.
4. **Transient (M3):** the adaptive trap/Gear-2 companion driver exists for linear
   circuits (#26) and its RC/RL/RLC/floating-capacitor/AC conformance gates pass
   (#48, including the `.ic`/`uic`/`ic=` fixtures of #27); more waveforms,
   higher-index DAEs (#29) and nonlinear initialization remain; bounded nonlinear
   charge is now provided by M4.
5. **Nonlinear devices (M4):** bounded equations, Newton/damping/continuation,
   typed nested sweeps and nonlinear DC/AC/charge-companion gate implemented;
   expansion beyond [M4_NONLINEAR.md](docs/port/M4_NONLINEAR.md) remains explicit work.
6. **Usability (M5):** CLI simulation, subcircuit flattening/instantiation (#18),
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
cargo xtask golden verify               # Rust vs C data: 16 verified, 4 unsupported
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
for terminal disambiguation. R/C/L also retain declared forward model references
and omitted values. Positioned PULSE/PWL, OFF/model-family flags and Q/M IC vectors
retain ordered setter semantics without enabling runtime support.
Parsing is not simulation: family/level checks and bounded
diode inputs belong to `spice-devices`; see
[MODEL_SCHEMAS.md](docs/port/MODEL_SCHEMAS.md) for APIs and explicit limits.

Winnow was the first external dependency and is used only by `spice-netlist`,
with its `std` and `parser` features. Petgraph is also used in production topology
APIs; faer supplies real/complex LU and diffsol supplies the bounded BDF backend.
Both solver backends are MIT-licensed; no LGPL KLU algorithms are copied.
Rust edition 2024 / MSRV **1.89** is required by the locked dependency graph.
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
xtask                   C capture/drift checks, Rust numerical verification, CI
conformance/            fixture decks and the rawfiles captured from ngspice
TODO.md                 central branch-aware implementation checklist
docs/port/              architecture, C-to-Rust mapping, roadmap, verification
```

## Verification

There is no FFI: the C implementation is used only as an oracle, out of process.
`conformance/netlists/` holds twenty decks: the original eight (an operating
point, an AC sweep, a transient run, a DC sweep, a diode, a BJT, a MOSFET and a
subcircuit) and eight M3 exit-gate decks (RL/RC/RLC/PWL transients with trapezoidal
and Gear-2 integration, floating and coupled capacitor networks, and an RLC AC
sweep) plus four initialized-state decks (`uic`/`ic=` RC, RLC and floating-capacitor
decays, and an `.ic` released after the initial bias);
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
the Rust simulation engine. `cargo xtask golden verify` runs the Rust library
APIs against three committed linear fixtures with name/metadata/axis/value
checks and explicit exclusions; no C binary is needed. Production DC/AC golden
comparisons and analytic/
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
