# `.four` — bounded Fourier/THD analysis

The M9 section below updates the historical subset descriptions in this guide.

`.four` is post-processing on a transient result: after a run, the deck's cards are
evaluated against the **full** plot the driver produced, before and independently of the
`.save`/`.print` selection that narrows what is written (exactly as `.measure` is,
[MEASURE.md](MEASURE.md)). A vector the output selection dropped is still transformed, and a
deck with a `.four` card writes the same rawfile it would have written without one.

There is **no FFT**: the final period is resampled onto a uniform grid and the harmonics are
quadrature projections of the trace onto that grid. The adaptive steps of the transient driver
are never treated as a uniform grid, and the port never transforms internal timestep indices.

| Piece | Where |
| --- | --- |
| Card grammar, typed requests, positioned errors | `src/netlist/parser/fourier.rs` |
| Card type and bounds (`DEFAULT_HARMONICS`, `MAX_HARMONICS`) | `src/netlist/ast.rs` |
| Evaluation, text block | `src/analysis/fourier.rs` |
| Full-plot evaluation before the `.save`/`.print` narrowing, report block | `src/cli/simulate.rs` |
| C reference | `src/frontend/fourier.c` (`fourier()`, `CKTfour()`), reached from `ft_cktcoms()` in `src/frontend/dotcards.c` |

## The supported grammar

```text
.four <fundamental-frequency> [HARMONICS=<n>] <vector> [<vector> …]

<fundamental-frequency> := a finite numeric literal greater than zero (1k, 2.5e3)
<n>                     := a whole number of harmonics in 1..=100
                           (DEFAULT_HARMONICS = 9 when HARMONICS= is absent)
<vector>                := v(<node>) | v(<first>,<second>) | i(<source|inductor|E|H>)
```

* `HARMONICS=` may appear anywhere after the frequency, once. `DEFAULT_HARMONICS = 9` gives the
  same table C builds from its `nfreqs` default of 10 rows (`src/frontend/fourier.c:69-70`): a
  DC row plus harmonics `1..=9`.
* The vector spelling is the `.save`/`.print` one, resolved through the *same*
  `selection::resolve_request` the written vectors use, so a measured/transformed vector and a
  written vector cannot disagree about differences, ground or units.
* `all` is not accepted (name one vector), and an AC component spelling (`vm(out)`, `vp`, `vr`,
  `vi`, `vdb`) is a parse error: those need a complex plot, and `.four` transforms a transient.
* The card applies to a `.tran` result only. A card in a deck that runs no
  `.tran` is `SpiceError::Unsupported` before anything runs, naming the analyses
  the deck does run. In a multi-analysis deck (#96) the cards are evaluated
  against the last executed `.tran` plot, which is the plot C's
  `setcplot("tran")` selects; see [CLI.md](CLI.md#multi-analysis-decks).
* A `.four` card inside a used `.subckt` definition is hoisted once, without vector renaming (M9 #108).

## The window

Every vector is transformed over the **final complete period** on the physical time axis:

```text
window = [to - 1/fundamental, to]        to = the plot's last time sample
```

This is C's default `nperiods = 1` window (`src/frontend/fourier.c:71-72`, `:135-141`: `dp[1]`
is the last time, `dp[0] = dp[1] - nperiods/fundamental`). The endpoint `to` is **included** as
the window's last grid point; C's own grid is half-open, which is one of the small differences
the live comparison sees. A run shorter than one period is refused rather than partially
transformed — C's `Error: (%d * wavelength) longer than time span`
(`src/frontend/fourier.c:137`).

## Resampling and quadrature

The window is resampled onto a **uniform closed grid** of `divisions + 1` points:

```text
t_i = from + i / fundamental / divisions         i = 0..=divisions
divisions = 4 * max(harmonics, 50)               (200 for the default 9 harmonics)
```

Each grid point is read with `.measure`'s sample model (`docs/port/MEASURE.md`): linear
interpolation inside the two samples that bracket it, and an explicit failure — never a silent
choice of one side — when it lands on a time carried by two samples with different values, i.e.
an unrepresented discontinuity. C instead interpolates with `polydegree = 1` by default onto
`fourgridsize = 200` points per period (`src/frontend/fourier.c:73-76`, `:142-148`).

With `w_i = i/divisions` the point's phase within the period, the coefficients are the
trapezoid quadrature of the resampled trace `y` against the sine and cosine basis:

```text
A_k = 2/divisions * Σ wgt_i * y_i * sin(2πk w_i)
B_k = 2/divisions * Σ wgt_i * y_i * cos(2πk w_i)      wgt_i = 1/2 at both ends, 1 inside
```

The discrete projection is exact below Nyquist for band-limited **grid values**; linear
interpolation from the original samples can still introduce error. With `divisions = 200`,
frequencies strictly below 100 cycles per period are below Nyquist. A square wave with its
edges aligned to the grid has amplitude bias `x/sin(x)-1`, where `x = πk/divisions`:
about 0.20 % at k=7 and 0.33 % at k=9. The bias grows as k²/divisions², not linearly with
harmonic order. Both engines use the same default resolution; all nine harmonics are
compared in the live-C test.

## Normalization, phase, DC and THD

| Quantity | Definition |
| --- | --- |
| Harmonic amplitude | `sqrt(A_k² + B_k²)` — the **single-sided peak amplitude** of the component `amplitude * sin(k * 2π f * (t - window.from) + phase)` in the vector's unit |
| Harmonic phase | `atan2(B_k, A_k)` in **radians** in `[-π, π]`; relative to `window.from`: phase `0` is a pure sine, `±π/2` a pure cosine |
| DC | the mean of the resampled trace over the window, `Σ wgt_i * y_i / divisions` (C's row `0`) |
| THD | `sqrt(Σ_{k≥2} (amplitude_k / amplitude_1)²)` — a **fraction** in the API, printed as a percentage in the text block (C's `thd = 100*sqrt(...)`, `src/frontend/fourier.c`) |

C prints the same phase in **degrees** (`atan2(cosine, sine)` scaled), so the opt-in comparison
converts once, explicitly. Like C, phase is referenced to the window's start, **not** absolute
time zero: stopping the same periodic signal a quarter period later shifts its reported
fundamental phase by π/2. A zero fundamental amplitude leaves the THD undefined and is
`SpiceError::Numerical`, never a fabricated `inf`.

## Failure policy

A Fourier result that cannot be computed is an error, never a fabricated `NaN`/`0`/clamped
value. Exit statuses are `simulate`'s (`docs/port/CLI.md`): `NotYetPorted` → 3, everything else
→ 2.

| Input | Class |
| --- | --- |
| malformed card, missing or non-positive fundamental, non-whole or repeated `HARMONICS=`, a value that is a `{…}`-free non-literal, a missing vector, `all`, an AC-component spelling, an unknown parameter | `SpiceError::Parse` (2), positioned at the offending token |
| `NFREQS=`, `NPERIODS=`, `POLYDEGREE=`, `FOURGRIDSIZE=` (C reads them from interactive `set` variables) | `SpiceError::NotYetPorted` (3) |
| a harmonic count beyond the port's resampling budget (`HARMONICS=101` with `MAX_HARMONICS = 100`), a card in a run that is not `.tran`, a plot without a `time` axis or with fewer than two points, a descending time axis, a run shorter than one period, a window with too few samples for the requested harmonics, a grid point on an unrepresented discontinuity, an unresolvable vector | `SpiceError::Unsupported` (2) |
| a non-finite axis, operand or result value, or a zero fundamental amplitude | `SpiceError::Numerical` (2) |

## Divergence from C

* **The grid is `4 * max(harmonics, 50)` subintervals per period.** This matches C's default
  resolution of 200 for counts up to 50, then grows to at most 400. C uses a half-open grid;
  the port includes both endpoints with trapezoid weights. For a non-periodic endpoint pair,
  these rules can differ. The port rejects `FOURGRIDSIZE=` instead of changing its grid.
* **`.four` does not add vectors to the rawfile selection.** C's `ft_savedotargs()` registers
  `.four` vectors into the TRAN save list (`src/frontend/dotcards.c:137-147`). The port
  evaluates the full plot independently of `.save`/`.print` and does not alter the rawfile.
* **`.four` is honoured when writing a rawfile.** C's `ft_cktcoms()` ignores `.four` when
  a batch run produces a rawfile (`src/frontend/dotcards.c:395-399`, the `terse` branch).
  The port writes the rawfile and prints the Fourier report on success.
* **Phase is radians.** C prints degrees.
* **THD is a fraction in the API** and a percentage in the text block; C only prints the
  percentage.
* **Explicit settings select a separate C-compatible path.** See the M9 settings
  section below; unconfigured `.four` retains the historical closed-grid behavior.
* **A failing card fails the run.** C prints `Error: …` to `cp_err` and carries on with the rest
  of the batch job; the port returns an error, publishes no report and leaves an existing
  output destination untouched, because a transform the port cannot compute is not a successful
  run. C's failure messages are modelled in the port's wording: `no vectors loaded`, `fourier
  needs real time scale`, `bad fundamental freq`, `isn't real!`, `longer than time span`
  (`src/frontend/fourier.c:65-137`).
* **A grid point on an unrepresented discontinuity is an error.** The port refuses to choose
  a side when duplicate time samples disagree; C delegates interpolation to `ft_interpolate`
  rather than exposing this same explicit duplicate-time error policy.
* **Only `.four` is a card name.** C accepts the `tran` analysis word and `.four`; other
  spellings reach it through the same `ciprefix`-style matching as `.measure`, which the port
  does not reproduce.

## Not ported

Inline `.four` setters `NFREQS=`, `NPERIODS=`, `POLYDEGREE=` and
`FOURGRIDSIZE=` remain unsupported; supply them through pre-run `set` or
`.options`. Non-transient transforms, `sp`/`sparam`, arbitrary interactive
commands and a full FFT library remain outside scope.

## Validation

* `src/analysis/fourier.rs` — unit tests over hand-built plots with analytic
  results: a pure sine and a known two-harmonic signal recover their amplitudes and phases, a DC
  offset is its own mean, the window/endpoint rules are pinned, and every failure class above has
  a test. Refusals cover a descending axis, a one-point plot, a short run, a harmonic count
  outside the budget, a non-finite sample and a zero fundamental.
* `tests/fourier_cards.rs` — the grammar, the positions, `DEFAULT_HARMONICS`
  and `MAX_HARMONICS`, the rejection classes, body-local cards, and the writer/semantic
  round-trip.
* `tests/simulate.rs` — process tests over a real PULSE square wave: the block
  lists every card, the source's own harmonics match the analytic `2/(kπ)` series, the filtered
  output is scaled by the analytic lowpass `|H(f)|` and delayed by `-atan(2πfRC)`, a vector the
  output selection dropped is still transformed, a deck without `.four` prints exactly what it
  printed before and writes the same rawfile, and a failing card publishes nothing.
* `tests/c_four_reference.rs` — the opt-in `#[ignore]`d comparison against
  the local C binary (`NGSPICE_BIN`), over the same deck for both engines: DC, THD and the
  harmonic table, for a filtered voltage, the unfiltered square wave and a branch current. It
  needs no C for ordinary runs.

## M9 explicit front-end settings (#115)

Bounded pre-run `set nfreqs=... nperiods=... polydegree=... fourgridsize=...`
is supported; the same numeric settings may be supplied with `.options` as a
Rust extension. `RunConfig::fourier_settings` exposes them and
`fourier::resolve_with_settings` evaluates them. Defaults are 10 rows including
DC, one period, degree 1 and 200 grid samples per period. Explicit `HARMONICS`
on a card overrides NFREQS. At least a fundamental is required for THD.

Explicit settings select C's half-open DFT grid over the last NPERIODS periods.
Degree zero deliberately follows C's sample-index DFT over the full trace,
without interpolation; it does not reinterpret adaptive samples as uniform
physical time. Degrees 1 through 16 use moving local polynomial stencils.
The configured interpolator follows C's absolute power-basis fits, acceptance
thresholds, degree fallback, moving stencil and duplicate-time edge nudging
(`maths/poly/interpolate.c`, `polyfit.c`). It also preserves C's final interval
coefficient-tail behavior after degree reduction. All harmonic magnitudes are
compared against C for degrees 0, 1, 2, 3, 5 and 7, as well as each configurable
setting. `resolve_with_stable_settings` is an explicit library alternative using
local Lagrange coordinates and rejecting repeated timestamps; it can differ
from C's high-degree fallback. Limits: 100 periods, 100 harmonics,
100000 grid samples per vector. Default unconfigured `.four` keeps the existing
closed trapezoid behavior documented above.

Fundamental frequencies accept `{expression}` and `'expression'`, resolved
against the top-level `.param`/`.func` scope before simulation. This is a Rust
extension: the local C reference rejects a braced `.four` frequency as “bad
fundamental freq”. A hoisted body card uses that same top-level scope, rather
than an arbitrary instance's parameter overrides. Library callers use
`fourier::resolve_cards`; analysis rejects unresolved expressions explicitly.

Bounded `.control` blocks accept pre-run `set`, one argument-free `run`,
`fourier <frequency> <vectors>` after that run, and a final argument-free `quit`.
The CLI evaluates the command against the last transient plot with configured
C-compatible defaults. A second run, settings after run, Fourier before run,
quit before run and commands after quit are explicit failures. Source round trips
and process tests cover this subset. Loading arbitrary plots, reruns and the
general interactive command interpreter remain unsupported.
