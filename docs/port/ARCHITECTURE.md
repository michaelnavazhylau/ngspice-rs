# Architecture of the Rust port

## Layering

Crates may only depend on crates **below** them. `spice-core` has no
dependencies at all; nothing depends on `spice-cli`.

```
                 spice-cli          (binary: netlist in, results out)
                     |
              spice-analysis         (op / dc / ac / tran, rawfile I/O)
                  /       \
        spice-devices     spice-maths   (MNA stamping | matrices, integrators)
                  \       /
               spice-netlist             (deck loading, tokenizer, AST, parser)
                     |
                spice-core               (numbers, units, nodes, errors)
```

| Crate | Responsibility | Mirrors |
| --- | --- | --- |
| `spice-core` | `Real`, `Complex`, SPICE numeric literals with scale factors, node table and ground aliasing, error type, analysis taxonomy | `src/include/ngspice/`, parts of `src/spicelib/parser/inpeval.c`, `src/frontend/inpcom.c` |
| `spice-netlist` | Deck loading (title line, `+` continuations, comments), tokenizer, card classification, target AST, parser | `src/frontend/inp.c`, `src/frontend/parse-bison.y`, `src/spicelib/parser/inp*.c` |
| `spice-maths` | Dense and sparse matrix storage, LU factorisation, numerical integration (trapezoidal / Gear) | `src/maths/dense/`, `src/maths/sparse/`, `src/maths/KLU/`, `src/maths/ni/` |
| `spice-devices` | `Device` trait, MNA stamping contract, device registry, `Circuit` container, built-in models | `src/spicelib/devices/` |
| `spice-analysis` | Analysis drivers, result plots, ASCII rawfile reading and writing | `src/spicelib/analysis/`, `src/frontend/rawfile.c` |
| `spice-cli` | Command-line entry point: `spice-rs <netlist>` | `src/frontend/main.c`, `src/ngspice.c` |
| `xtask` | Automation: golden-data capture from the C binary, drift checks, CI | — |

## Design rules

1. **The C tree is the specification.** Every non-trivial behaviour carries a
   doc comment naming the C file and function it must match. When the C code is
   ambiguous, the doc comment says so instead of guessing.
2. **Unimplemented means loud.** Stubs return
   `SpiceError::NotYetPorted { what, c_reference }`; `todo!()`/`unimplemented!()`
   are denied by clippy at the workspace level. `spice-cli` maps that error to
   exit status `3`, so scripts can distinguish "not ported yet" from a real
   failure.
3. **No FFI in the port.** The C library is only ever reached out-of-process, by
   `xtask` driving the `ngspice` binary to produce comparison data. A Rust
   `unsafe_code = "forbid"` workspace lint enforces this.
4. **Differentiable at the token level.** The tokenizer keeps the exact source
   text and column of every token, so parser errors can be reported against the
   original deck.
5. **Numbers are `f64` until proven otherwise.** ngspice mixes `double` and
   `float`; the port uses `Real = f64` and records any place where the C code
   loses precision in `float` as a documented divergence risk.

## Two data models for a netlist

`spice-netlist` distinguishes:

- **`RawCard`** — a logical line plus its token stream and a coarse `CardKind`
  classification (`Device { designator }`, `DotCommand(..)`). Produced by the
  tokenizer; already implemented.
- **`Netlist`** — the semantic model (device instances with typed parameters,
  `.model` cards, `.subckt` bodies, analyses, includes). Produced by the parser;
  **not implemented**.

Keeping both means the front-end can be ported incrementally: classification and
tokenization are useful on their own (the CLI can report what a deck contains
before any parser exists), and the semantic model can evolve without breaking
the tokenizer's contract.

## Deliberate divergences from the C code

Tracked here so they are never mistaken for bugs. Each also appears as a doc
comment at the divergence site.

| Divergence | Reason |
| --- | --- |
| `parse_spice_number_prefix()` consumes `MEG`/`MIL` in full, while `INPevaluate()` leaves those letters unconsumed | `INPevaluate()` returns a value and a rest pointer; consumers (`inpcom.c` scale scanning, `INPevaluateRKM_*`) do the skipping themselves. The Rust API reports bytes consumed, so it consumes the whole recognised suffix. The numeric result is identical. |
| RKM-style literals (`4k7` meaning `4.7k`, `inp2r.c`/`inp2c.c`/`inp2l.c`) are **not** accepted | Not ported yet; `parse_spice_number("4k7")` returns `None`. A unit test pins this so the change is deliberate. |
