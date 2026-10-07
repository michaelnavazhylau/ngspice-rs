# The `spice-rs` command line

The CLI is a thin front end over the production engine: it parses the deck with
`spice_netlist::Parser`, elaborates it and runs the analysis with the
`spice-analysis` drivers. No device, solver or rawfile logic lives here. The C
equivalents are `src/frontend/main.c`, the batch path of `src/ngspice.c`
(`CKTdoJob()`) and `raw_write()` in `src/frontend/rawfile.c`.

## Commands

| Command | What it does |
| --- | --- |
| `spice-rs <netlist>` (default) | prints a summary of the deck and what the port can do with it |
| `spice-rs cards <netlist>` | lists every card with its classification |
| `spice-rs tokens <netlist>` | dumps the token stream |
| `spice-rs parse <netlist>` | builds the semantic netlist and reports unported gaps |
| `spice-rs simulate --output <path> <netlist>` | runs the deck's single analysis and writes an ASCII rawfile |
| `spice-rs devices` | lists the device designators the registry knows |
| `spice-rs analyses` | lists the analyses and their driver status |
| `spice-rs help`, `spice-rs version` | usage and version |

`--no-auto-gnd` (treat `gnd` as an ordinary node, C's `no_auto_gnd` front-end
variable) reaches the parser for every command that reads a deck. It does not
change the device node table: `Circuit` always builds its `NodeTable` with
aliasing on, so a deck using `gnd` folds to `0` either way. That is pre-existing
`spice-devices` behaviour, not something `simulate` chooses. `--output` is
accepted by `simulate` only, and is required there.

## `simulate`

```
spice-rs simulate --output <path> [--no-auto-gnd] <netlist>
spice-rs simulate --output=<path> [--no-auto-gnd] <netlist>
```

The command, in order:

1. loads and parses the deck (`Parser::with_auto_gnd`, so `--no-auto-gnd`
   reaches the parser);
2. requires **exactly one** analysis card (`.op`, `.dc`, `.ac` or `.tran`) and
   rejects zero or several requests explicitly. Zero analyses is an input
   failure (exit 2); more than one is exit 3, because a deck with several
   analyses is valid input that C runs and the port simply has no scheduler for
   yet, so the missing capability is a documented port gap rather than a
   malformed command line or deck;
3. resolves the deck's `.option` cards through `RunConfig::from_netlist`, which
   rejects unknown and not-yet-implemented settings before anything runs;
4. elaborates the circuit at the configured temperatures and runs the production
   driver for that analysis;
5. only then writes the plot as an ASCII rawfile at `<path>`.

An unadorned `.tran` runs the companion trapezoidal/Gear-2 driver the engine
defaults to; `simulate` never injects `backend=diffsol`, and an explicit
`backend=diffsol method=bdf` on the card is passed through unchanged (and fails
where that backend does not apply, e.g. with `uic` or `.ic`). See
[TRANSIENT.md](TRANSIENT.md).

### The rawfile

The file is ngspice's ASCII form (`set filetype=ascii`), written by
`spice_analysis::RawFile::write`:

* `Title:` is the deck's title line, `Command:` is
  `spice-rs <version> (Rust port), Build`, `Date:` is the write time in UTC in
  standard `ctime` spelling. It is not byte-identical to ngspice's
  `datestring()`, which writes local time and leaves an extra pre-year space;
  no committed comparator reads `Date:`;
* a `.dc` plot's scale column is named `sweep` (C spells it `v(v-sweep)` or
  `i(i-sweep)`; `cargo xtask golden verify` maps the name when it compares
  against the committed C goldens);
* binary rawfiles are not supported and are neither written nor read.

### Output-path and overwrite contract

* `<path>` is written through a temporary file in the **same directory**
  (`.NAME.spice-rs-PID-NANOS.tmp`) that is renamed into place only after the
  whole rawfile was written. A failed run therefore never truncates, replaces
  or removes an existing destination and never leaves a partial rawfile. The
  temporary file is removed on every failure path that can run, so only process
  death or a failing cleanup leaves one behind. The rename cannot cross a
  filesystem boundary, so no partial copy is possible.
* An existing file at `<path>` **is** replaced, but only by a successful run.
* Because the destination is replaced rather than opened in place, a symbolic
  link at `<path>` is replaced by the new regular file instead of being written
  through.
* An output path that names no file (`.`, `..`, `/`), whose directory does not
  exist, or that cannot be written, is an output failure (exit 2). An empty
  `--output` value (`--output=""` or `--output=`) is a usage error (exit 1),
  like a missing one.

`simulate` never prints a partial success: on any error nothing is written to
`<path>` and only a diagnostic goes to stderr. On success it reports the deck,
title, analysis, plot, variable names and output path on stdout.

## Exit status

| Status | Meaning |
| --- | --- |
| 0 | success |
| 1 | bad command line (unknown option, missing or empty `--output`, unexpected argument) |
| 2 | the deck, the run or the output failed: unreadable/unparsable deck, no analysis requested, numerical failure, unsupported analysis kind or device, missing output directory |
| 3 | the requested operation is a documented gap in the port (`SpiceError::NotYetPorted`): an option that is known but unimplemented, an unported model family or device grammar, more than one analysis card in one deck |

## What is deliberately not implemented

* waveform parsing and any other post-processing of results;
* binary rawfile output (ASCII only);
* output selection / save-set control (every result vector the driver produces
  is written);
* an interactive interpreter;
* more than one analysis per invocation.

## Covered by tests

`crates/spice-cli/tests/simulate.rs` runs the binary and checks the exit
contract, the exactly-one-analysis rule and the write/rename guarantee; it reads
the written rawfiles back with the production reader and compares them vector by
vector, by name, with the committed C goldens in `conformance/golden/`
(`.op`, both `.dc` sweep directions, complex `.ac`, plain `.tran` and a
`uic`/`ic=` `.tran`). `crates/spice-cli/tests/parse.rs` keeps the older
inspection commands green. Nothing in these tests invokes C or re-captures a
golden.
