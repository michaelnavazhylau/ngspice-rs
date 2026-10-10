# Output selection: `.save` and `.print`

The M9 section below updates the historical subset descriptions in this guide.

Implemented for issue #42 in the Rust-only `work/m5-save-print` worktree, based on
`origin/main` `1334358`. The C reference is `src/frontend/dotcards.c`
(`ft_dotsaves()`, `ft_savedotargs()`, `fixem()`), `src/frontend/breakp2.c`
(`com_save()`/`settrace()`, the `dbs` save list) and `src/frontend/outitf.c`
(`beginPlot()`, which turns the save list into the plot's vectors). C is never
linked or called by the port.

Route map:

| Item | Where |
| --- | --- |
| `.save`/`.print` parsing (typed, positioned requests) | `src/netlist/parser/save.rs` |
| Request AST (`SaveCard`, `PrintCard`, `RequestedVector`, `OutputCards`) | `src/netlist/ast.rs` |
| Parser entry point that returns them | `Parser::parse_file_with_output`, `Parser::parse_deck_with_output` (`src/netlist/parser.rs`) |
| Selection over a full `Plot`, and the text rendering | `src/analysis/selection.rs` |
| CLI wiring (`simulate`) | `src/cli/simulate.rs` |
| Unit tests over synthetic plots | `src/analysis/selection.rs` (`mod tests`) |
| Parser tests | `tests/save_print.rs` |
| Process tests (`.op`/`.dc`/`.ac`, text, failures) | `tests/simulate.rs` |

## What ngspice does

* `.save` is **deck-wide**: `ft_dotsaves()` collects every `.save` line and hands
  each word to `com_save()`, which normalises the word with `copynode()`
  (`v(2)` → node `2`, `i(vds)` → the `vds#branch` vector) and stores it in the
  `dbs` list. With a rawfile (`-r`), only `.save` narrows it.
* `.print <analysis> …` is **analysis-specific**: `ft_savedotargs()` reads the
  analysis name and registers the rest of the line for that analysis. In batch
  mode without a rawfile those words also become the save set, and
  `ft_cktcoms()` prints one table per `.print` line for the matching plot. With
  a rawfile, `.print` is ignored ("`.print` line ignored since rawfile was
  produced").
* `beginPlot()` starts from the reference vector — `time` for `.tran`,
  `frequency` for `.ac`, the swept source for `.dc` (`refName` is `NULL` for
  `.op`, so an operating-point plot has none) — and then appends the saves that
  match the running analysis, in save-list order, skipping names the analysis
  does not have data for.
* `fixem()`/`fixdotprint()` rewrite the AC component spellings:
  `vm(a,b)` → `mag(v(a)-v(b))`, and `vp`/`vr`/`vi`/`vdb` likewise; a `,0`
  terminal drops out of the difference.

M8 adds named analysis vectors (`S_2_1`, `Y_1_1`, `NF`,
`onoise_spectrum`, `onoise_total`) and `mag`/`ph`/`real`/`imag`/`db`
components of named vectors. Missing names still fail at resolution.
`.print noise` routes spectra and integrated totals to their respective
plots. `keepopinfo` bias plots receive deck-wide saves, and no SP/noise print
or measurement cards. See [SPARAM.md](SPARAM.md) and [NOISE.md](NOISE.md).

## The supported request grammar

```
.save  <request>...
.print <analysis> <request>...

<request>  := all
            | v( <node> )
            | v( <node> , <node> )
            | i( <device> )
            | <component> ( <node> )
            | <component> ( <node> , <node> )
<component> := vm | vp | vr | vi | vdb
<analysis>  := op | dc | ac | tran | noise | disto | pz | sens | tf | four
```

* node names are canonicalised like device nodes: lowercased, and `gnd` becomes
  `0` under the default `auto_gnd` rule (`Parser::with_auto_gnd(false)` keeps it
  a plain node). Matching against the plot is case-insensitive.
* device names are lowercased, so `.save i(V1)` matches the plot's `i(v1)`.
* `i(…)` selects existing branch currents or requests a device terminal current.
  See the M9 observation section below for supported analyses and limits.
* `vm` is the magnitude, `vp` the phase in radians in `(-pi, pi]` (C's `ph()`),
  `vr`/`vi` the real and imaginary parts, `vdb` `20*log10` of the magnitude.

Everything outside that grammar is a positioned failure, never a dropped or
synthesised request:

| Input | Result |
| --- | --- |
| Unsupported device current asks | `SpiceError::NotYetPorted` (exit 3); see M9 device observations below |
| Unsupported `@device[param]` asks | `SpiceError::NotYetPorted` (exit 3); supported scalar asks are listed below |
| `power(v1)`, `im(v1)`, a bare `v`, `v(a,b,c)`, a missing `)`, an empty `v()` | `SpiceError::Parse` (exit 2) |
| `v(a,a)`, `v(0,0)` | `SpiceError::Parse` (exit 2): identically zero, so there is nothing to write |
| `.save`/`.print` with no requests | `SpiceError::Parse` (exit 2) |
| `.print` without a valid analysis name (`v(out)`, `tranx`) | `SpiceError::Parse` (exit 2) |
| `.print`/`.plot` inside a `.subckt` body | `SpiceError::NotYetPorted` (exit 3); body `.save` is supported by M9 expansion |

Every message carries the offending token's `SourceLoc` (`deck.cir:6:7`).

## Semantics

**Default.** A deck with no `.save` and no `.print` card writes exactly the
driver's plot: same variables, same order, same values, byte for byte what the
command wrote before this issue. The `save_all_writes_exactly_the_default_rawfile`
process test runs the same deck with and without `.save all` and compares the
two rawfiles ignoring only the `Date:` header.

**`all`.** Any `all` request (or an empty request list) keeps the driver's whole
vector set in the driver's order: `all` cancels narrowing. `.save all v(out)`
is therefore the default output, not "`v(out)` only".

**Ordering.** A selection only ever narrows, and writes in C's `dbs` order:
every `.save` request in card order, then every applicable `.print` request in
card order. A sweep keeps its independent vector first (C's `beginPlot()` pass
0): `.dc` keeps `sweep`, `.ac` keeps `frequency`, `.tran` keeps `time`. An
operating-point result has no independent vector, so only the requests are
written. Requests keep their order even when the driver orders the same columns
differently: `.save v(out) v(in)` writes `v(out)` then `v(in)`, while the
unselected plot has `v(in)` first.

**Duplicates.** A request whose resolved vector is already written is dropped,
first occurrence wins. Two different spellings of one vector count as
duplicates: `.save v(out) v(out,0)` writes one `v(out)` column, because
`v(out,0)` resolves to the same signed sum. `v(out)` and `vm(out)` are different
vectors and both survive. Duplicates are not errors: C collapses them the same
way, silently.

**Differences.** `v(a,b)` is `v(a) - v(b)`. Ground is not a column (ground is
0 V by definition), exactly as C's `fixem()` rewrites the terminals:
`v(a,0)` is `v(a)`, `v(0,a)` is `-v(a)`, and `v(0)` is an all-zero column.
`v(a,a)` and `v(0,0)` are rejected as identically zero.

**Currents.** `i(v1)`/`i(l1)` copy the driver's own column, so the sign
convention is unchanged: C prints `i(v1)` as the current *into* the positive
terminal, and the committed goldens pin it (5 V across a 2 k divider gives
`i(v1) = -2.5 mA`). Selection never flips or recomputes a branch current.

**Missing, ambiguous, unsupported.** All of them fail *before* anything is
published, and an `all` request does not excuse the others: `.save all v(nosuch)`
is still an error, because a request the user wrote is never silently dropped.

* a request the full plot cannot satisfy (`.save v(nosuch)`, `.save i(l9)`, a
  `.print op …` where the deck runs `.ac`) is `SpiceError::Unsupported` (exit 2).
  The message names the request, its `SourceLoc` and the vectors the analysis
  actually carries;
* an AC component asked of a real plot (`vm(out)` on a `.op`/`.dc`/`.tran`
  result) is `SpiceError::Unsupported` (exit 2): the port does not pretend a
  real result has a phase;
* a `.print` card naming an analysis type the deck does not run is
  `SpiceError::Unsupported` (exit 2) rather than a silent no-op: such a card can
  never be honoured. In a multi-analysis deck (#96) a `.print <type>` card
  applies to every plot of that type and to no other plot, and `.save` applies
  to every plot; see [CLI.md](CLI.md#multi-analysis-decks);
* a computed vector that is not finite (`vdb` of a zero magnitude) is
  `SpiceError::Numerical` (exit 2), so a selection never puts `inf` or `NaN`
  into a rawfile.

In every case nothing is printed on stdout and an existing `--output`
destination is left untouched, because the selection is resolved before the
report and before the rawfile write.

**Complex format.** The text table prints computed components, not the rawfile's
bytes: `%.15e`, and in a complex plot `re,im` for a column with a non-zero
imaginary part and a single real number for a column that is real at every point
— which is C's `print` behaviour, and what the AC components (`vm`, `vp`, `vr`,
`vi`, `vdb`) produce. The rawfile spells such a column `re,0.0` instead (the same
rule the writer already applies to `frequency`), so the table and the file agree
on values but not on that spelling. The table states the convention on its second
line, e.g.

```
print: 6 vector(s): frequency v(out) vm(out) vp(out) vdb(out) i(v1)
values: complex as `re,im` with 15 fractional digits, except that a component which is real for every point prints as one number (C's print behaviour; the rawfile spells that column `re,0.0`); vm is |v|, vp is the phase in radians in (-pi, pi], vr/vi are the real and imaginary parts, vdb is 20*log10|v|; computed from the plot, not the rawfile's own spelling
  point  frequency  v(out)  vm(out)  vp(out)  vdb(out)  i(v1)
      0  1.000000000000000e+02,0.000000000000000e+00  7.169568003248978e-01,-4.504772433683886e-01  …
```

**The full plot is kept.** The selection is a projection applied only to what is
written. `Selection::apply` builds a new plot; the driver's plot stays
untouched for the measurement work that follows (`simulate` keeps it in scope
until after the rawfile is written).

## Divergence from C

* **A rawfile run still honours `.print`.** C with `-r` ignores `.print`
  entirely and narrows the rawfile by `.save` alone. The port keeps the `.print`
  selection because it is the only way `simulate` can honour the card at all: it
  narrows what is written (never widens it) and additionally prints the table.
* **A request that cannot be resolved is an error, even next to `all`.** C warns
  and skips a `.save` name the analysis has no data for (`outitf.c:391-517`),
  including when `all` is also present; this port rejects it, because issue #42
  requires unknown expressions not to be ignored.
* **`v(a,a)` and `v(0,0)` are errors.** C's `fixem` rewrites `v(0,0)` to `v(0)`
  and silently ignores `v(a,a)` (`dotcards.c:524-554`); the port reports both as
  identically-zero requests instead of writing a column that can only be zero.
* **A `.print` card for an analysis the deck does not run is an error.** C
  prints `Error: .print: no <type> analysis found.` and carries on; this port
  rejects the card, because a dropped request is exactly the failure mode issue
  #42 is about.
* **A plot with no applicable request is written whole.** Without `-r`, C's
  batch path also turns `.print`/`.op`/`.four`/`.measure` operands into
  analysis-specific save entries, so a plot whose type none of them names keeps
  only its scale (and `beginPlot()` reports "no data saved" for it). The port
  narrows a plot only by `.save` and by `.print` cards of its own type (#96).
* **Body `.save` requests are translated per instance.** See the M9 scoped-card
  rules below; `.print`/`.plot` inside bodies remain unsupported.

## Public API

```rust
// netlist
Parser::parse_file_with_output(path) -> SpiceResult<ParsedDeck>
Parser::parse_deck_with_output(&deck) -> SpiceResult<ParsedDeck>
pub struct ParsedDeck { pub netlist: Netlist, pub output: OutputCards }
pub struct OutputCards { pub saves: Vec<SaveCard>, pub prints: Vec<PrintCard> }
pub struct SaveCard  { pub requests: Vec<VectorRequest>, pub location: SourceLoc }
pub struct PrintCard { pub analysis: AnalysisKind, pub analysis_location: SourceLoc,
                       pub requests: Vec<VectorRequest>, pub location: SourceLoc }
pub struct VectorRequest { pub vector: RequestedVector, pub location: SourceLoc }
pub enum RequestedVector { All, Voltage { positive: NodeName, negative: Option<NodeName> },
                           Current { device: String },
                           Component { component: VectorComponent, positive: NodeName,
                                       negative: Option<NodeName> } }
pub enum VectorComponent { Magnitude, Phase, Real, Imaginary, Decibels }

// analysis
selection::write_requests(&OutputCards, AnalysisKind) -> SpiceResult<Vec<VectorRequest>>
selection::print_requests(&OutputCards, AnalysisKind) -> SpiceResult<Vec<VectorRequest>>
Selection::resolve(&Plot, AnalysisKind, &[VectorRequest]) -> SpiceResult<Selection>
Selection::is_full(&self) -> bool
Selection::variable_names(&self) -> Vec<&str>
Selection::apply(&self, &Plot) -> SpiceResult<Plot>
Selection::to_text(&self, &Plot) -> SpiceResult<String>
```

`Selection` is a pure function of a plot and requests, so both the projection
and the text rendering are tested with hand-built `Plot`s, without a CLI or a
solver. The output cards are returned beside the `Netlist` rather than inside
it: they describe an analysis' output, not the circuit, and keeping them out of
`Netlist` means no `devices` or rawfile-layer change was needed and every
existing consumer of `Netlist` is untouched.

## Known limits

* ASCII `.plot` and device observations have the bounded M9 support described
  below; unsupported rendering options and device asks fail explicitly.
* A `.dc` scale column is named `sweep` by the port's driver (C writes
  `v(v-sweep)`); a selection keeps whatever the driver produced, and the
  committed-golden comparator maps the name (see [CLI.md](CLI.md)).
* The text table is a bounded rendering of the selected vectors: no `xlimit`,
  no column-width formatting options, and no interactive `print`/`plot`
  commands.

## M9 ASCII plots (#111)

`.plot <analysis> <operands...>` now uses the same parser, target validation,
full-plot resolution and rawfile selection as `.print`. It renders a line-printer
graph with C's `+*=$%!0123456789` legend and `X` for collisions. The independent
axis runs down the page. Each row echoes its physical scale and the first trace
value. Complex operands use their real component; use `vm`, `vp`, etc. to select
another component. Nonfinite data, missing operands and descending axes fail
before any publication.

Formatting deliberately uses a fixed 60-column field and the solver's physical
sample rows rather than C's terminal widths, page breaks, date headings and
resampled rows. Live C process checks compare legend identities and DC physical
rows, not byte-for-byte pagination. Graphical backends, explicit plot limits and
interactive plotting options remain unsupported.

### M9 subcircuit front-end cards (#108)

Subcircuit bodies retain typed `.option`, `.global`, `.ic`, `.nodeset`,
`.save`, `.measure` and `.four` cards, with their ordered source cards and
positions. The writer preserves their definition scope. Expansion follows C's
`frontend/inp.c::inp_spsource`, `frontend/subckt.c::translate` and
`collect_global_nodes`:

- Used definitions contribute global nodes before instance node translation;
  unused definitions contribute none.
- Options and node hints follow expanded card order. Local expressions use each
  instance's parameter scope. Hint nodes and save voltage/current names are
  translated through ports, globals and instance paths.
- Measurements repeat for every instance, retaining their result and vector
  names. A bare internal node in a body measurement consequently still names a
  top-level vector; an absent vector fails explicitly.
- Fourier cards are hoisted once per used definition before expansion, with
  their original vector names. Multiple instances do not multiply transforms.

`devices::subckt::expand_with_output` returns the expanded output requests for
library consumers; the CLI applies them before target validation. `RunConfig`
applies instantiated options and hints. `.print`/`.plot` inside bodies, nested
subcircuit definitions and body analysis requests remain explicit unsupported
cases. Expansion limits also bound front-end cards in otherwise empty bodies.

`m9_frontend` covers syntax round trips, local parameters, precedence, unused
bodies, limits and atomic failures. Its opt-in C check verifies renaming,
measurement repetition, saved variables and Fourier hoisting. The new committed
`m9_scoped_frontend` C rawfile compares the resulting RC circuit on a shared
physical time grid; existing goldens were not regenerated.

## M9 device observations (#113)

`.save`, `.print`, `.plot`, `.measure` and `.four` may request `i(device)`
for a non-branch device or `@device[param]`. Requested quantities are registered
before solving and added to the full plot before output selection. Existing
source/inductor branch columns retain their signs and names. Body saves rename
both current requests and `@instance` names per instance.

`Circuit::set_observations` and `Circuit::observe_real` expose the device-side
API. Scalar asks use `Device::observation_parameter`, following `*ask.c`.
Real terminal currents use a disposable physical device load at the solved
point, with limiting disabled, the point's model context, source forcing and
pre-acceptance integration history. Observing never accepts state. Ground
terminal current follows terminal KCL. Two-terminal power is voltage drop times
current. DC parameter/source sweeps are observed before restoring their context;
companion transient currents include the integrated charge contribution.

Supported scalar asks include R/C/L values and applicable geometry, multiplicity,
temperature and instance fields for D/Q/MOS1. Unsupported keywords fail with a
`NotYetPorted` error naming the C ask routines. AC supports scalar parameter asks;
AC current/power asks, coincident terminal currents and diffsol observations are
explicitly refused. Other analysis drivers that cannot emit an observation fail
when resolving the requested vector. These bounds do not imply every upstream
ask or model is implemented.

`m9_observations` compares saved R/D/Q/M currents and parameters against the
new committed `conformance/observations/m9_device_dc.raw` C fixture. The opt-in
C test checks that fixture without recapture. Analytic RC transient tests check
capacitor/resistor KCL, power and measurement integration; invalid requests
preserve an existing output file.
