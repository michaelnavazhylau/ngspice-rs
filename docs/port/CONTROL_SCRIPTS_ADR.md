# ADR #116: a bounded `.control` block, not a command interpreter

Status: **accepted; the bounded subset is implemented** (configurable Fourier and
the front-end settings landed with #115, the encoding precedence with #112). A
full command interpreter is out of scope by this decision, which is what the
issue asked to record.

Code: `src/netlist/parser/scopes.rs` (block scoping and the accepted commands),
`src/netlist/parser/options.rs::frontend_setting` (the seven settings),
`src/netlist/parser/grammar.rs` (`parse_frontend_setting`), `src/analysis/fourier.rs`
(`fourier` → `.four`), `src/cli/simulate.rs` (the encoding precedence).
Tests: `tests/linear_parser.rs` (`quit` before `run`), `tests/m8_additional_outputs.rs`
(`set sqrnoise`, `set ngbehavior=s3`, `resume`, an unknown setting, a nested
block), `tests/m9_fourier.rs` (`set nfreqs…`, `run`, `fourier`, `quit`, a
repeated `run`, commands after `quit`), `tests/simulate.rs` (`set filetype`
beating `SPICE_ASCIIRAWFILE`), `tests/m9_frontend.rs` (the C-comparison of the
front-end translation the subset feeds), plus the conformance snapshots.

## Context: what C's `.control` is

C reads a `.control` … `.endc` section as **script text**, not as circuit cards.
`src/frontend/inp.c` collects the lines, `control.c` sets up the interpreter
state, and `runcoms.c` dispatches the first word of each line against the
`com_*` table (`plot`, `print`, `display`, `let`, `set`, `option`, `run`,
`resume`, `quit`, `source`, `cd`, `shell`, `meas`, `fourier`, `fft`, `write`,
`hardcopy`, `postscript`, `repeat`, `if`, `while`, `foreach`, `echo`, `listing`,
`strcmp`, …). Commands evaluate expressions through `inpcom.c`'s ptree
machinery, assign to a variable namespace, and act on the *current plot*: C
builds up in-memory plots as analyses run and lets the script inspect, transform
and re-plot them. In batch mode (`-b`) the block runs after the deck's analyses
(it is what most ngspice regression decks use to write rawfiles and compare
results), and in interactive mode the same commands form the REPL.

That is a second language with ~100 commands, side effects on the filesystem
(`source`, `cd`, `write`, `hardcopy`) and the process table (`shell`), and its
own variable and plot state.

## Decision

The port implements the **declarative subset a batch deck needs to configure an
analysis**, and refuses everything else by name:

Accepted inside a `.control` … `.endc` block:

| command | rule |
| --- | --- |
| `set <option>…` | seven settings only: `sqrnoise`, `ngbehavior`, `filetype`, `nfreqs`, `nperiods`, `polydegree`, `fourgridsize`; **before** `run` |
| `run` | once, no arguments; the deck's own analysis cards select what it runs |
| `fourier …` | after `run`; parsed as `.four` and evaluated over the transient result |
| `quit` | after `run`, and nothing after it |

Refused with `SpiceError::NotYetPorted` and the C reference (`src/frontend/inp.c`,
`runcoms.c`, `variable.c`, `control.c`): any other command, `set` after `run`,
`run` twice, argument-bearing `run`/`quit`, `quit` before `run`, commands after
`quit`, a nested or repeated `.control`/`.endc`, and a `set` whose name is not
one of the seven. The block itself is position-independent, as in C.

The block is a *settings block*, not a program: it cannot compute, branch,
loop, name a variable or touch a plot. Everything a script would compute, the
port's CLI does instead (`--format`, `--output`, `.save`/`.print` for output
selection, `.measure`/`.four` for post-processing, the exit status contract).

## Divergences from C

- **Order is checked, not executed.** C's interpreter runs the lines in order
  with full state; the port's parser accepts a canonical order and rejects the
  rest. A deck that C runs happily, such as one that `set`s a Fourier variable
  after `run`, is refused with a message naming the command.
- **No variables, expressions or control flow.** `let`, `if`, `while`,
  `repeat`, `foreach` and every arithmetic expression are refused, even where C
  would accept them trivially.
- **No plotting or inspection.** `plot`, `print`, `display`, `show`, `meas`,
  `fft`, `spec` and friends are refused; the port's own output paths
  (`.print`, `.save`, `--format`, the rawfile) cover the batch use case.
- **No filesystem or process access.** `source`, `cd`, `shell`, `write`,
  `hardcopy` and `postscript` are refused; the port has no interactive surface
  and no reason to gain one.
- **C's `set` is a variable store** with a large namespace; the port's seven
  settings are validated options with defined precedence (a control `set
  filetype` beats `SPICE_ASCIIRAWFILE`, which the CLI's `--format` beats).

## Why not a full interpreter

1. **No oracle.** C's interpreter output is interactive text; the port's tests
   compare plots, rawfiles and exit statuses. A command loop would need a
   per-command C-parity argument and a second test harness for text output.
2. **It duplicates existing machinery.** Expression evaluation already exists
   for `netlist::eval` (parameters, `.param`, behavioural sources) and output
   selection for `analysis::selection`; a scripting layer would either reuse
   them (a larger surface than any single analysis) or diverge from them.
3. **It conflicts with the port's posture.** `shell`/`source`/`cd` reach
   outside the deck, while the port's contract is one deck in, one
   deterministic output out; the repository forbids the process and FFI
   surface those commands imply.
4. **The batch use case is already covered.** Configure (the seven settings),
   run once, post-process (`.measure`, `.four`, `.print`) and stop is what
   regression decks do; a script that needs more needs an interpreter, not a
   better settings block.

## What a full interpreter would require

If the port ever needs one, the pieces are: a command loop with C's `com_*`
dispatch table and its abbreviations; a variable namespace and `let`; an
expression evaluator (reusing `netlist::eval`'s parser and the primitive
types); plot and vector commands over the existing `Plot` and
`analysis::selection` structures, including `setplot`/`display`/`print`; file
commands (`source`, `write`, `load`) over the existing deck loader and rawfile
code; control flow (`if`/`while`/`repeat`/`foreach`) with C's error semantics;
and an explicit decision about `shell`/`cd`, which the port would refuse. Each
of those is larger than the analyses this repository has been shipping, which
is why this ADR records the bounded subset rather than a roadmap to one.

## Verification

The subset is pinned by the tests listed at the top: the accepted commands
(`tests/m8_additional_outputs.rs`, `tests/m9_fourier.rs`, `tests/simulate.rs`),
the refusals (`tests/linear_parser.rs`, `tests/m9_fourier.rs`,
`tests/m8_additional_outputs.rs`) and the C-side equivalence of the settings
they drive (`tests/m9_frontend.rs`, `tests/c_noise_reference.rs`, the Fourier
goldens). The CLI's own documentation states the same boundary in
[CLI.md](CLI.md#what-is-deliberately-not-implemented).
