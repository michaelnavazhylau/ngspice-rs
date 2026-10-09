# The `spice-rs` command line

The CLI is a thin front end over the production engine: it parses the deck with
`netlist::Parser`, elaborates it and runs the analysis with the
`analysis` drivers. No device, solver or rawfile logic lives here. The C
equivalents are `src/frontend/main.c`, the batch path of `src/ngspice.c`
(`CKTdoJob()`) and `raw_write()` in `src/frontend/rawfile.c`.

## Commands

| Command | What it does |
| --- | --- |
| `spice-rs <netlist>` (default) | prints a summary of the deck and what the port can do with it |
| `spice-rs cards <netlist>` | lists every card with its classification |
| `spice-rs tokens <netlist>` | dumps the token stream |
| `spice-rs parse <netlist>` | builds the semantic netlist and reports unported gaps |
| `spice-rs simulate --output <path> <netlist>` | runs every analysis of the deck in ngspice batch order and writes one ASCII rawfile with a plot per analysis |
| `spice-rs devices` | lists every device designator as `ported` (built from its card), `bounded` (built from a deck for a stated subset: D/Q/M, S/W, X) or `pending` (`NotYetPorted` with its C reference) |
| `spice-rs analyses` | lists the analyses: `.op`/`.dc`/`.ac`/`.tran`/`.pz`/`.tf` drivers, `.four` as a post-processor of the `.tran` plot, the rest without a driver |
| `spice-rs help`, `spice-rs version` | usage and version |

Both tables are derived, not hand-maintained (#117): a designator's status
comes from the factory module's own designator lists (`DeviceSupport::of`), and
`tests/registry_support.rs` elaborates a representative deck for
every designator to check that `ported` and `bounded` devices build and
`pending` ones fail with `NotYetPorted`. Analysis status comes from
`analysis::support`.

`--no-auto-gnd` (treat `gnd` as an ordinary node, C's `no_auto_gnd` front-end
variable) reaches the parser for every command that reads a deck. It does not
change the device node table: `Circuit` always builds its `NodeTable` with
aliasing on, so a deck using `gnd` folds to `0` either way. That is pre-existing
`devices` behaviour, not something `simulate` chooses. `--output` is
accepted by `simulate` only, and is required there.

## `simulate`

```
spice-rs simulate --output <path> [--no-auto-gnd] <netlist>
spice-rs simulate --output=<path> [--no-auto-gnd] <netlist>
```

The command, in order:

1. loads and parses the deck (`Parser::with_auto_gnd`, so `--no-auto-gnd`
   reaches the parser);
2. requires **at least one** analysis card; zero analyses is an input failure
   (exit 2);
3. resolves the deck's `.option` cards through `RunConfig::from_netlist`, which
   rejects unknown and not-yet-implemented settings before anything runs;
4. schedules every analysis card in ngspice batch order
   ([multi-analysis decks](#multi-analysis-decks)), builds and validates every
   request and driver, elaborates the circuit at the configured temperatures and
   checks that every `.print`/`.measure`/`.four` card targets an analysis the
   deck runs — all before the first analysis starts;
5. runs each analysis through the production driver on a freshly elaborated
   circuit and resolves its `.save`/`.print`/`.measure`/`.four` outputs against
   the full plot;
6. only when every analysis and every output card succeeded, writes all plots as
   one ASCII rawfile at `<path>`.

An unadorned `.tran` runs the companion trapezoidal/Gear-2 driver the engine
defaults to; `simulate` never injects `backend=diffsol`, and an explicit
`backend=diffsol method=bdf` on the card is passed through unchanged (and fails
where that backend does not apply, e.g. with `uic` or `.ic`). See
[TRANSIENT.md](TRANSIENT.md).

### The rawfile

The file is ngspice's ASCII form (`set filetype=ascii`), written by
`analysis::RawFile::write`:

* `Title:` is the deck's title line, `Command:` is
  `spice-rs <version> (Rust port), Build`, `Date:` is the write time in UTC in
  standard `ctime` spelling. It is not byte-identical to ngspice's
  `datestring()`, which writes local time and leaves an extra pre-year space;
  no committed comparator reads `Date:`. `Title:` keeps the deck's own casing
  where C lowercases it; no committed comparator reads `Title:` either;
* a `.dc` plot's scale column is named `sweep` (C spells it `v(v-sweep)` or
  `i(i-sweep)`; `cargo xtask golden verify` maps the name when it compares
  against the committed C goldens);
* a multi-analysis deck writes one plot per analysis, concatenated in batch
  order, each with the same three headers (the layout `raw_write()` produces for
  several plots, and what `RawFile::parse` reads back);
* `simulate` always writes ASCII; it has no binary output. The rawfile library
  itself reads *and* writes binary real/complex files since #45
  ([RAWFILES.md](RAWFILES.md)); exposing a `simulate` format flag is future work.

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
title, analysis, plot, variable names and output path on stdout. A
multi-analysis deck reports the deck, title and output, a `plots:` line with the
C plot names in rawfile order, and one `[name]` section per plot with its
analysis, plot and variable lines followed by that plot's `.print`, `.measure`
and `.four` blocks. A single-analysis deck's report is unchanged.

### Multi-analysis decks

A deck may contain any number of `.op`, `.dc`, `.ac`, `.tran`, `.pz` and `.tf` cards
(GitHub #96, #101). `simulate` reproduces `ngspice -b -r <path> deck.cir`, which runs them all
in one job; the scheduling rules live in `analysis::batch`:

* **Order.** `CKTdoJob()` (`src/spicelib/analysis/cktdojob.c`) walks the fixed
  analysis table `analInfo[]` (`analysis.c`): `.ac`, then `.dc`, then `.op`, then
  `.tran`, then `.tf`, and `.sp` (`SPinfo`, an `RFSPICE` entry) after every
  other type — not deck order. Cards of one type run in **reverse deck order**,
  because `CKTnewAnal()` prepends each job to the task's list. A deck written
  `.tran .ac .dc a .op .dc b` therefore runs `.ac`, `.dc b`, `.dc a`, `.op`,
  `.tran`, and the rawfile holds the plots in that order.
* **Plot names.** The rawfile carries each plot's `Plotname:` (`AC Analysis`,
  `DC transfer characteristic`, `Operating Point`, `Transient Analysis`,
  `Transfer Function`, `SP Analysis`) and the
  deck title on every plot. The report also gives each plot the name C's
  `plot_add()` (`src/frontend/vectors.c`) gives it in memory: the type
  abbreviation plus the global `plot_num`, which a name collision bumps for good —
  the deck above yields `ac1 dc1 dc2 op2 tran2`.
* **Output cards per analysis type.** `.save` applies to every plot. `.print
  <type>` narrows and prints a table for every plot of that type and no other;
  a plot whose type has no `.print` and no `.save` is written whole. `.measure
  <type>` cards are evaluated against the **last executed** plot of their type,
  and `.four` against the last `.tran` plot (C's `plot_cur` and
  `setcplot("tran")` after the run). A `.print`, `.measure` or `.four` card for
  an analysis type the deck does not run is an explicit error before anything
  runs.
* **Independent analyses.** Each analysis starts from a freshly elaborated
  circuit, so no device state carries from one analysis to the next. C reuses
  one circuit (only its Newton starting guess carries over), which does not
  change a converged result.
* **Atomic publication.** Every analysis runs and every output card resolves
  before anything is printed or written: one failing analysis, selection,
  measurement or Fourier card publishes nothing, and an existing `<path>` is
  left untouched.
* **Deck options apply to every request.** `RunConfig::request` adds the deck's
  `.option` settings to each analysis exactly as for a single-analysis deck, so
  a DC continuation option (`itl1`, `itl2`, `srcsteps`, `gminsteps`,
  `gminfactor`) in a deck that also runs `.tran` bounds the companion `.tran`
  initial bias as well (#110); `backend=diffsol` still rejects them explicitly.

Divergences from C, all deliberate:

* C keeps running the remaining analyses after one fails and leaves the plots
  that succeeded in the rawfile; the port publishes nothing.
* C with `-r` ignores `.print`, `.four` and `.measure` entirely (see
  [OUTPUT_SELECTION.md](OUTPUT_SELECTION.md), [MEASURE.md](MEASURE.md),
  [FOURIER.md](FOURIER.md)); without `-r`, `dosim()` evaluates only the
  `.measure` cards whose type matches the **last** analysis that ran and skips
  the others silently. The port evaluates every `.measure` card against the
  last plot of its own type instead of dropping it.
* C prints `Error: .print: no ac analysis found.` and carries on for a `.print`
  card naming an analysis the deck does not run; the port rejects the card.

`tests/c_batch_reference.rs` is the opt-in check that ties
these rules to real batch output: it runs `ngspice -b -r` (binary rawfile) and
`spice-rs simulate` on the committed `multi_analysis_rc` fixture and on a deck
with two `.dc` cards, and requires the same plot count, order, names, flags,
variables and values:

```sh
NGSPICE_BIN=/abs/path/ngspice cargo test -p ngspice-rs --test c_batch_reference -- --ignored
```

## Exit status

| Status | Meaning |
| --- | --- |
| 0 | success |
| 1 | bad command line (unknown option, missing or empty `--output`, unexpected argument) |
| 2 | the deck, the run or the output failed: unreadable/unparsable deck, no analysis requested, numerical failure, unsupported analysis kind or device, missing output directory |
| 3 | the requested operation is a documented gap in the port (`SpiceError::NotYetPorted`): an option that is known but unimplemented, an unported model family or device grammar |

## What is deliberately not implemented

* waveform parsing and any post-processing of results other than `.measure`;
* binary rawfile output (ASCII only);
* an interactive interpreter (`run`/`resume`, `.control` sections).

Output selection is implemented for the bounded `.save`/`.print` subset: a deck's
`.save` (deck-wide) and `.print <analysis> …` (analysis-specific) cards narrow
which vectors `simulate` writes, and `.print` also renders a text table on
stdout. A deck without those cards writes exactly the driver's plot, unchanged.
See [OUTPUT_SELECTION.md](OUTPUT_SELECTION.md) for the grammar, the ordering and
duplicate rules and the failure modes.

Fourier/THD reporting is implemented for the bounded `.four` subset, evaluated over
the final complete period of the transient result, and its block is appended after
the `.measure` block. See [FOURIER.md](FOURIER.md).

Measurements are implemented for the bounded `.measure`/`.meas` subset: the
cards are evaluated over the **full** plot before the selection narrows what is
written, so an operand the selection dropped is still measurable, and the
measurement block is appended to the report after the `.print` table. A deck
without a `.measure` card prints exactly what it printed before, and no
measurement changes the written rawfile. See [MEASURE.md](MEASURE.md) for the
grammar, the axis/interpolation/crossing/window rules and the failure modes.

## Covered by tests

`tests/simulate.rs` runs the binary and checks the exit
contract, the multi-analysis batch order, C plot names, per-analysis output
cards and atomic failure (against the multi-plot golden `multi_analysis_rc`),
and the write/rename guarantee; it reads
the written rawfiles back with the production reader and compares them vector by
vector, by name, with the committed C goldens in `conformance/golden/`
(`.op`, both `.dc` sweep directions, complex `.ac`, plain `.tran` and a
`uic`/`ic=` `.tran`). It also covers `.save`/`.print` selection and its failures
(see [OUTPUT_SELECTION.md](OUTPUT_SELECTION.md)), `.measure` measurement
blocks, hidden operands and failed measurements (see [MEASURE.md](MEASURE.md)), and
`.four` Fourier/THD blocks (see [FOURIER.md](FOURIER.md)).
`tests/parse.rs`
keeps the older inspection commands green. Nothing in these tests invokes C or
re-captures a golden.
