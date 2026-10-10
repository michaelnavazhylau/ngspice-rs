# Measurements: `.measure` and `.meas`

Implemented for issue #43 in the Rust-only `work/m5-measure` worktree, based on
`origin/main` `7624471` (which already carries `.save`/`.print` output selection,
subcircuit elaboration, the `simulate` CLI and binary rawfile I/O). The C
reference is `src/frontend/com_measure2.c` (`get_measure2()` and its helpers),
driven by `do_measure()` (`src/frontend/measure.c`); the deck's `.meas` lines are
collected out of the deck by `inp_spsource()` (`src/frontend/inp.c:1221`). C is
never linked or called by the port; the opt-in live comparison runs it as a
separate process (see [Testing](#testing)).

Route map:

| Item | Where |
| --- | --- |
| `.measure`/`.meas` parsing (typed, positioned requests) | `src/netlist/parser/measure.rs` |
| Request AST (`MeasureCard`, `MeasureRequest`, `MeasureEvent`, `MeasureWindow`) | `src/netlist/ast.rs` |
| Parser entry point that returns them | `Parser::parse_deck_with_output` / `parse_file_with_output`, field `ParsedDeck::measurements` |
| Evaluation over a full `Plot`, and the text rendering | `src/analysis/measure.rs` |
| Shared operand resolution (`v(a,b)`, ground, AC components) | `src/analysis/selection.rs` (`resolve_request`, `Column`) |
| CLI wiring (`simulate`) | `src/cli/simulate.rs` (`Report::measured`, `Report::measurements`) |
| Unit tests over synthetic plots | `src/analysis/measure.rs` (`mod tests`) |
| Parser tests | `tests/measure_cards.rs` |
| Process tests (report, hidden operand, failures) | `tests/simulate.rs` |
| Opt-in live-C comparison (`#[ignore]`d, `NGSPICE_BIN`) | `tests/c_measure_reference.rs` |

## What ngspice does

* `.measure`/`.meas` cards are **removed from the deck** and kept per circuit
  (`ft_curckt->ci_meas`), then evaluated after the run by `do_measure()`. The
  cards themselves name their analysis (`tran`/`dc`/`ac`/`sp`); `do_measure()`
  only evaluates the cards whose analysis word matches the plot that ran.
* Measurements read the vectors of the **current plot** (`plot_cur`), after the
  run and independently of what a rawfile received. C's own rule is narrower
  than the port's: `.meas` is refused outright when batch mode was started with
  `-b` **and** `-r` (`measure.c:240`).
* The independent axis is the plot's scale vector (`plot_cur->pl_scale`): `time`
  for a transient, `frequency` for AC, the swept vector for DC.
* `measure_at()` interpolates linearly between the two samples bracketing a
  queried time; `com_measure_when()` classifies each sample as above or below
  the threshold and interpolates the crossing linearly; `measure_minMaxAvg()`
  clips an `AVG` window to exactly `[from, to]` (Enhancement-302) while `MIN` and
  `MAX` keep whole samples; `measure_rms_integral()` mixes Simpson 3/8, Simpson
  1/3 and the trapezoid rule, but only where three consecutive sample widths are
  equal to within 100 ULPs (`com_measure2.c:1504`), which an adaptive transient
  grid essentially never satisfies.
* Results are printed as `%-20s=  %.*e` with `measure_get_precision()` (5 by
  default), plus the operation's own fields (`at=`, `from=`/`to=`,
  `targ=`/`trig=`).

## The supported grammar

```
card      := .measure | .meas
.measure <analysis> <name> <request>

<analysis> := tran | ac | dc                       (an optional leading '.' is accepted)

<request>  := FIND <operand> AT=<value> [FROM=<value>] [TO=<value>]
            | MIN|MAX|AVG|RMS|INTEG|INTEGRAL <operand> [FROM=<value>] [TO=<value>]
            | TRIG <event> TARG <event>

<event>    := AT=<value>
            | <operand> VAL=<value> [<selector>] [FROM=<value>] [TO=<value>]

<selector> := RISE=<n> | FALL=<n> | CROSS=<n> | LAST
            | RISE=LAST | FALL=LAST | CROSS=LAST          (n >= 1 is a whole number)

<operand>  := v(<node>) | v(<node>,<node>) | i(<source|inductor|E|H>)
            | vm|vp|vr|vi|vdb (<node> [, <node>])         (the `.save` spelling, without `all`)

<value>    := a finite numeric literal, e.g. 1m, 2.5e-3
```

* `<analysis>` is the analysis the measurement applies to, exactly as C's card
  spells it. The port requires it (C guesses `tran` when it is missing) and
  requires the deck to run that analysis type: a `.measure ac …` card in a deck
  without `.ac` can never be honoured and is rejected before anything runs. In
  a multi-analysis deck (#96) a card is evaluated against the **last executed**
  plot of its own type (C's `plot_cur`); unlike C's `dosim()`, which evaluates
  only the cards whose type matches the last analysis that ran and skips the
  rest silently, the port evaluates every card. See
  [CLI.md](CLI.md#multi-analysis-decks).
* `<name>` is the result name; it must be a word (a bare number is rejected) and
  is kept as written. Names may repeat across cards: every card produces its own
  result, in card order, exactly as C prints one line per card — names are never
  merged or overwritten.
* `<operand>` is the same bounded vector spelling as `.save`/`.print`, resolved
  by the same code (`selection::resolve_request`): nodes are canonicalised
  (`gnd` → `0` under the default `auto_gnd` rule, case-insensitive matching),
  `v(a,b)` is `v(a) - v(b)`, ground is not a column, `i(…)` accepts a voltage
  source or an inductor, and `vm`/`vp`/`vr`/`vi`/`vdb` are the AC components
  (`vp` in radians, as in the port's `.print` table).
* `FROM`/`TO` are the axis window; a `TRIG`/`TARG` card may write them in either
  clause, and two clauses that disagree are an error.
* Every card is validated at parse time and typed; nothing is stored as a
  string to be re-parsed later.

### Rejections

| Input | Result |
| --- | --- |
| missing/invalid analysis word (`.measure`, `.measure 2 …`) | `SpiceError::Parse` (exit 2), positioned |
| a measurement over an S-parameter plot (`sp`, `sparam`) | frequency axis; named vectors and `mag`/`ph`/`real`/`imag`/`db` components |
| an analysis no measurement can be taken on (`op`, `noise`, `disto`, `pz`, `sens`, `tf`, `four`) | `SpiceError::Unsupported` (exit 2) |
| a C operation the port does not implement (`WHEN`, `MIN_AT`, `MAX_AT`, `PP`, `DERIV[ATIVE]`, `ERR*`, `PHASE_MARGIN`, `GAIN_MARGIN`) | `SpiceError::NotYetPorted` (exit 3) |
| an unknown operation word | `SpiceError::Parse` (exit 2) |
| `TD=<value>` | `SpiceError::NotYetPorted` (exit 3): measurements start at the axis origin |
| `AT=`/`VAL=` on `MIN`/`MAX`/`AVG`/`RMS`/`INTEG` (C stores and ignores them) | `SpiceError::Parse` (exit 2) |
| `VAL=` on `FIND`, or `FIND … WHEN …` | `SpiceError::Parse` / `SpiceError::NotYetPorted` |
| `all` as an operand | `SpiceError::Parse` (exit 2) |
| a repeated setter, two crossing selectors, `RISE=0`/`FALL=0`/`CROSS=0`, a non-numeric or `{…}` value, a missing `=` | `SpiceError::Parse` (exit 2), except a `{…}` value which is `SpiceError::NotYetPorted` |
| a `.measure` card inside a `.subckt` body | `SpiceError::NotYetPorted` (exit 3): body-local measurement scope is not defined yet |
| an operand the plot cannot satisfy (`v(nosuch)`, `i(r1)`, `vm(out)` of a real plot) | `SpiceError::Unsupported` (exit 2), as for `.save`/`.print` |

Every parse-time message carries the offending token's `SourceLoc`
(`deck.cir:6:14`); every evaluation-time message carries the card's location.

## The axis

The axis is the driver's scale vector of the plot, addressed by name:
`time` for `.tran`, `frequency` for `.ac`, `sweep` for `.dc` — the same columns
`beginPlot()` puts first and that the rawfile writes first.

* The axis must be **finite** and **non-decreasing**. A descending axis (a
  descending `.dc` sweep, e.g. `dc v1 2 0 -0.5`) or a nested `.dc` sweep (whose
  scale column restarts at every outer point) is `SpiceError::Unsupported`, never
  measured in traversal order.
* A plot with no such axis (an operating-point or other non-sweep result, or a
  plot whose scale is named differently) is `SpiceError::Unsupported`; an
  operand that the plot lacks reports the analysis' actual vector list.
* A deck with no `.measure` card is never validated against an axis, so an
  `.op` run without measurement cards is unaffected.

## Interpolation

The axis and the operand are piecewise linear **over the plot's own samples**:

* a query strictly inside a bracket `[x_i, x_{i+1}]` with `x_i < x_{i+1}` uses
  linear interpolation;
* a bracket of zero width (`x_i == x_{i+1}`) is a **jump**: two samples share an
  axis value but not an operand value.

The port may interpolate inside a bracket without crossing a hidden
discontinuity because the drivers guarantee a sample on every source breakpoint:

* the companion driver lands a step exactly on every breakpoint and keeps every
  accepted time point, so "no sample ever interpolates across a breakpoint and
  every breakpoint inside the run has a sample; ... the sample at a breakpoint is
  the left limit and the next sample is the first step of the right-hand
  segment" (`src/analysis/companion.rs:50-56`);
* the diffsol segments likewise break at every source breakpoint and restart the
  state from the right (`src/analysis/transient.rs`, the
  `DaeSegment` loop);
* the forcing rule itself never spans a jump: transient loads evaluate a step
  that ends at a breakpoint with `Limit::Left` and the next one with
  `Limit::Right` (`src/devices/linear.rs:4-19`).

For a plot that did **not** come from these drivers (a hand-built or imported
plot), the samples are the only evidence available, and the rule above is
applied to them unchanged: the port never assumes a discontinuity that the plot
does not show, and never interpolates across one that it does (a duplicated axis
value). A jump therefore behaves as follows:

| Query | Behaviour |
| --- | --- |
| `FIND … AT=<t>` at a jump time `t` whose left and right limits differ | `SpiceError::Unsupported`: the value at a discontinuity is not defined (C would divide by a zero span and report "out of interval") |
| `FIND … AT=<t>` at a duplicated time whose two values are equal | the value, as written |
| a crossing of a threshold inside the jump bracket | the crossing's axis position **is** the jump time, interpolated exactly, with no division |
| an integral or mean whose boundary is a jump time | the boundary is a sample, so nothing is interpolated across it |

## Crossings

`TRIG`/`TARG` events and `RISE`/`FALL`/`CROSS`/`LAST` selectors follow C's
`com_measure_when()`:

* the operand is on the **high** side of the threshold when
  `value >= VAL` (touching the threshold counts as high, as C's section test
  does);
* a transition between consecutive samples from one side to the other is one
  crossing; the direction is `RISE` (low → high) or `FALL` (high → low);
* the crossing's axis position is `x_i + (x_{i+1} - x_i) * (VAL - y_i) /
  (y_{i+1} - y_i)`, or the shared axis value when the bracket is a jump. Both
  denominators are non-zero by construction (the samples are on opposite sides of
  the threshold, and a zero-width bracket takes the jump branch);
* `RISE=n`/`FALL=n`/`CROSS=n` take the `n`-th crossing of that direction
  (`CROSS` counts both) with `n >= 1`; `LAST` takes the last crossing in either
  direction; with no selector the first crossing in either direction is taken;
* the scan starts at the last sample at or before the window's lower bound, so
  the side the operand is on as it enters the window is known, and the crossing
  between that sample and the first in-window sample is still found (as in C);
* asking for a crossing that does not occur is `SpiceError::Unsupported` naming
  the selector, the operand, the threshold and the window — never `NaN`.
* The result of a `TRIG … TARG …` card is `targ - trig` (C's `AT_DELAY`); both
  event positions are reported (`targ=`/`trig=`), and a negative distance
  (a target before the trigger) is reported as computed.

## Windows

* `FROM`/`TO` default to the axis' first/last value. Both are finite literals.
* `FROM > TO` is `SpiceError::Unsupported`: the port does not silently swap an
  inverted window (C does, for `.dc`) and does not treat `TO=0` as "no upper
  bound" (C's sentinel).
* A window that does not overlap the axis (`[5, 6]` on a `[0, 1ms]` transient) is
  `SpiceError::Unsupported`; a window that reaches past the axis is clipped to
  it, and a mean or integral covers exactly the clipped window (`from=`/`to=` in
  the report).
* A window that contains no sample, or whose width is zero, is
  `SpiceError::Unsupported` **for `AVG`/`RMS`/`INTEG`**, whose value is a mean or
  an integral over the width: an `AVG` of an empty window is not a number the
  port is willing to invent, and C's division by a zero span is not reproduced.
  `MIN`/`MAX` reduce whole samples instead, so they are defined on a window that
  contains a sample even when `FROM == TO`; a `MIN`/`MAX` window that contains no
  sample is the "covers no sample" error above.
* `FIND … AT=` and `TRIG/TARG … AT=` must lie inside both the axis and the
  window; anything else is `SpiceError::Unsupported` ("out of interval",
  C's wording).
* `MIN`/`MAX` reduce whole samples inside the window (C's semantics): the
  boundaries are not interpolated for them, so their value is one the plot
  actually carries. Ties keep the last sample that attained the extremum, as
  C's `value <= mValue` / `value >= mValue` do, and the extremum's axis position
  is reported (`at=`).

## Arithmetic

| Operation | Rule | Unit |
| --- | --- | --- |
| `FIND` | the operand at one axis value | the operand's |
| `MIN`/`MAX` | whole samples in the window | the operand's |
| `AVG` | trapezoidal integral over the window ÷ the window's width | the operand's |
| `RMS` | √(trapezoidal integral of the square over the window ÷ the width) | the operand's |
| `INTEG` | trapezoidal integral over the window | `<operand>*<axis>` |
| `TRIG/TARG` | `targ - trig` | the axis' |

The trapezoid rule is exact for linear data and is the rule C uses for `AVG`;
the port uses it for `INTEG` and `RMS` too, where C only reaches for Simpson's
rules on uniformly spaced samples (see above). `AVG` and
`INTEG/(to - from)` therefore agree exactly, which is C's own Enhancement-302
contract.

Every value the arithmetic touches comes from the **full** plot through the
shared operand resolution, so a vector that the `.save`/`.print` selection
dropped is still measured, and every result is checked to be finite:

* a non-finite operand value at a point the measurement uses is
  `SpiceError::Numerical`;
* a non-finite result (an overflowing square, a sum that overflows) is
  `SpiceError::Numerical`;
* neither is ever written to stdout or to a rawfile.

## Result shape and text

```rust
pub struct Measurement {
    pub name: String,
    pub value: Real,
    pub unit: String,                    // `voltage`, `current`, `phase`, `db`, `voltage*time`, `time`, …
    pub at: Option<Real>,                // FIND and MIN/MAX
    pub window: Option<MeasureSpan>,     // AVG/RMS/INTEG: the covered window
    pub events: Option<MeasureEvents>,   // TRIG/TARG: both axis positions
}

pub fn analysis::measure::resolve(
    plot: &Plot, kind: AnalysisKind, cards: &[MeasureCard],
) -> SpiceResult<Vec<Measurement>>;
pub fn analysis::measure::to_text(results: &[Measurement]) -> String;
```

The CLI appends the block after the report (and after a `.print` table):

```
measure: 3 result(s), evaluated on the full plot before any .save/.print selection
vmax                =  9.932620524394554e-01 at=  5.000000000000000e-03
vavg                =  8.013474895320336e-01 from=  0.000000000000000e+00 to=  5.000000000000000e-03
tdelay              =  6.931471670199225e-04 targ=  6.931476670199224e-04 trig=  5.000000000000000e-10
```

The layout follows C's `%-20s=  %.*e` plus the operation's own fields; the
numbers use the port's 15-fractional-digit spelling instead of C's 5.

**A deck without a `.measure` card is untouched.** `measure::resolve` returns an
empty list before it looks at the plot, `Report::measured` stays `None`, and
`report_text` adds nothing: the stdout and the rawfile of such a deck are exactly
what they were before this work (`a_measure_card_is_measured_over_the_full_plot_and_leaves_the_rawfile_alone`
compares the rawfiles of the same deck with and without the card, ignoring only
the `Date:` header).

## Testing

* `src/analysis/measure.rs` unit-tests hand-built plots with
  analytic results: constants, ramps (with the trapezoid rule's own O(h²) error
  stated), sinusoids (RMS `1/√2`, zero mean, crossings at 7/12 and 11/12 of a
  period), crossing counts/`LAST`, window boundaries off the samples, empty and
  out-of-range windows, zero-width windows, duplicate times at jumps, missing
  operands and axes, descending axes, non-finite operands and results, and the
  exact text block.
* `tests/measure_cards.rs` covers the grammar: every
  supported operation, both card spellings, positions, selectors, the rejection
  matrix, body-local cards, and the writer/`semantic_eq` round trip.
* `tests/simulate.rs` runs the binary: the measurement block,
  the unchanged rawfile/stdout without a card, an operand the selection dropped,
  and the "a failed measurement publishes nothing" contract (empty stdout, exit
  2 or 3, the destination untouched).
* `tests/c_measure_reference.rs` is the opt-in live-C
  comparison (`.tran` delay/rise/statistics, `.ac` magnitude/dB, `.dc` values and
  integrals). It is `#[ignore]`d and driven by `NGSPICE_BIN`:

  ```
  NGSPICE_BIN=/path/to/ngspice cargo test -p ngspice-rs \
      --test c_measure_reference -- --ignored
  ```

  The comparison tolerance (3e-5 relative) is a few of C's printed ULPs, because
  C prints measurements with five fractional digits. Ordinary `cargo test` never
  runs it, and it reads and writes no committed golden.

## Divergence from C

* **A failed measurement fails the run.** C prints
  `measure <name> failed!` for the failing card and still writes its results;
  the port returns `SpiceError::Unsupported`/`NotYetPorted`/`Numerical` and
  publishes nothing (no stdout, destination untouched), because a measurement
  the port cannot compute is not a successful run. The exit status separates
  "not ported" (3) from "failed" (2).
* **`.measure` works next to a rawfile.** C refuses every `.measure` when batch
  mode is started with `-b -r` (`src/frontend/measure.c:240`); `simulate` always
  writes a rawfile, so it evaluates the cards over the full plot and never
  narrows the rawfile because of them.
* **The analysis word is required and must match.** C guesses `tran` when the
  analysis word is missing and evaluates a card for another analysis as a
  failure inside that card; the port rejects the card up front, positioned.
* **`AT=`/`VAL=` on a statistic is an error.** C parses and ignores them; the
  port refuses to silently ignore a parameter. The same applies to a
  `RISE=`/`FALL=`/`CROSS=`/`LAST` selector on `FIND` or on a statistic: it
  describes an event, so it is a positioned `Parse` error rather than a silently
  dropped word.
* **An inverted or zero-bounded window is an error.** C swaps `FROM`/`TO` for
  `.dc` and treats `TO=0` as "no upper bound"; the port requires `FROM <= TO`,
  treats every value literally, and reports the covered window, so the echoed
  `from=`/`to=` always describe the window the value came from.
* **A query at a discontinuity is an error.** C interpolates with a zero span
  (`NaN`, reported as "out of interval"); the port names the discontinuity.
* **A sample exactly on the threshold starts no new crossing.** The port's
  section test is "the high side changed between consecutive samples", so a
  plateau whose samples all equal `VAL` after a rise is one crossing, not a fall
  and a second rise. C's `com_measure2.c:677-694` fires a fall for a sample that
  equals `VAL` while its section is `ABOVE`, which then divides by
  `value - prevValue == 0` in the position formula (`NaN`, "out of interval");
  the port keeps the well-defined answer and documents the difference, so a
  `CROSS=2`/`FALL=1` selector over such a plateau is an explicit "no crossing"
  error here where C reports a degenerate event.
* **The crossing that enters the window may lie below `FROM`.** The scan starts at
  the last sample at or before `FROM` so the side the operand enters on is known
  and a transition that began just before the window is still found; the reported
  position is the interpolated crossing of that bracket, which can be slightly
  below `FROM`. This is C's behaviour, and it is what makes a `TRIG` that began
  before the window detectable at all.
* **`vp` is in radians.** C's measurement `get_value()` returns degrees for `vp`;
  the port uses the same radians convention as its `.print` table, so a measured
  and a printed phase agree (the `vdb`/`vm`/`vr`/`vi` spellings agree with C).
* **`INTEG`/`RMS` use the trapezoid rule.** C mixes Simpson 3/8, Simpson 1/3 and
  the trapezoid rule, but only inside runs of uniformly spaced samples, which an
  adaptive grid essentially never produces: on the port's own plots C takes the
  trapezoid branch anyway. The test tolerances state the remaining difference.
* **Only `.measure`/`.meas` are card names.** C matches `.meas` as a prefix
  (`ciprefix(".meas", …)` in `inp_spsource()`), so `.measure`, `.meas` and even
  `.measurement` reach the measurement path there; the port accepts the two
  spellings its card classifier knows and rejects anything else as an unknown
  directive.
* **A `.measure` card inside a `.subckt` body is not ported** (C hoists such a
  card out of the body into `ci_meas`); the port rejects it explicitly rather
  than guessing the scope.
* **`TD=`, `WHEN`, `MIN_AT`/`MAX_AT`, `PP`, `DERIV`, `ERR*` and the margin
  measurements are not ported** and are rejected with `NotYetPorted` naming
  `src/frontend/com_measure2.c`, never silently ignored.

M8 SP measurements use `frequency`, including C's `vm(S_2_1)`/`vr(S_2_1)`
vector aliases. The port additionally accepts explicit named-vector components
such as `mag(S_2_1)`/`real(S_2_1)` directly; C's `.meas` parser requires the
`vm`/`vr` aliases instead of those expression spellings. `c_measure_reference`
compares the aliases against C's printed measurements.
