# `.four` — bounded Fourier/THD analysis

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
| Card grammar, typed requests, positioned errors | `crates/spice-netlist/src/parser/fourier.rs` |
| Card type and bounds (`DEFAULT_HARMONICS`, `MAX_HARMONICS`) | `crates/spice-netlist/src/ast.rs` |
| Evaluation, text block | `crates/spice-analysis/src/fourier.rs` |
| Full-plot evaluation before the `.save`/`.print` narrowing, report block | `crates/spice-cli/src/simulate.rs` |
| C reference | `src/frontend/fourier.c` (`fourier()`, `CKTfour()`), reached from `ft_dorun()` in `src/frontend/dotcards.c` |

## The supported grammar

```text
.four <fundamental-frequency> [HARMONICS=<n>] <vector> [<vector> …]

<fundamental-frequency> := a finite numeric literal greater than zero (1k, 2.5e3)
<n>                     := a whole number of harmonics in 1..=100
                           (DEFAULT_HARMONICS = 9 when HARMONICS= is absent)
<vector>                := v(<node>) | v(<first>,<second>) | i(<source|inductor>)
```

* `HARMONICS=` may appear anywhere after the frequency, once. `DEFAULT_HARMONICS = 9` gives the
  same table C builds from its `nfreqs` default of 10 rows (`src/frontend/fourier.c:69-70`): a
  DC row plus harmonics `1..=9`.
* The vector spelling is the `.save`/`.print` one, resolved through the *same*
  `selection::resolve_request` the written vectors use, so a measured/transformed vector and a
  written vector cannot disagree about differences, ground or units.
* `all` is not accepted (name one vector), and an AC component spelling (`vm(out)`, `vp`, `vr`,
  `vi`, `vdb`) is a parse error: those need a complex plot, and `.four` transforms a transient.
* The card applies to a `.tran` run only. A card in a run that is not `.tran` is
  `SpiceError::Unsupported` at evaluation, naming the analysis that ran.
* A `.four` card inside a `.subckt` body is rejected like `.save`/`.print`.

## The window

Every vector is transformed over the **final complete period** on the physical time axis:

```text
window = [to - 1/fundamental, to]        to = the plot's last time sample
```

This is C's default `nperiods = 1` window (`src/frontend/fourier.c:71-72`, `:105-110`: `dp[1]`
is the last time, `dp[0] = dp[1] - nperiods/fundamental`). The endpoint `to` is **included** as
the window's last grid point; C's own grid is half-open, which is one of the small differences
the live comparison sees. A run shorter than one period is refused rather than partially
transformed — C's `Error: (%d * wavelength) longer than time span`
(`src/frontend/fourier.c:137`).

## Resampling and quadrature

The window is resampled onto a **uniform closed grid** of `divisions + 1` points:

```text
t_i = from + i / fundamental / divisions         i = 0..=divisions
divisions = 4 * max(harmonics, 16)               (64 for the default 9 harmonics)
```

Each grid point is read with `.measure`'s sample model (`docs/port/MEASURE.md`): linear
interpolation inside the two samples that bracket it, and an explicit failure — never a silent
choice of one side — when it lands on a time carried by two samples with different values, i.e.
an unrepresented discontinuity. C instead interpolates with `polydegree = 1` by default onto
`fourgridsize = 200` points per period (`src/frontend/fourier.c:73-76`, `:126-129`).

With `w_i = i/divisions` the point's phase within the period, the coefficients are the
trapezoid quadrature of the resampled trace `y` against the sine and cosine basis:

```text
A_k = 2/divisions * Σ wgt_i * y_i * sin(2πk w_i)
B_k = 2/divisions * Σ wgt_i * y_i * cos(2πk w_i)      wgt_i = 1/2 at both ends, 1 inside
```

The rule is exact for a trace whose content stops below `divisions/2` cycles per period: with
`divisions = 64`, harmonics up to 32 are integrated exactly for a band-limited trace. A trace
with a genuine discontinuity (a square wave) is integrated only to the grid's own accuracy,
which is why the live comparison bounds the higher harmonics of an ideal step separately.

## Normalization, phase, DC and THD

| Quantity | Definition |
| --- | --- |
| Harmonic amplitude | `sqrt(A_k² + B_k²)` — the **single-sided peak amplitude** of the component `amplitude * sin(k * 2π f t + phase)` in the vector's unit |
| Harmonic phase | `atan2(B_k, A_k)` in **radians** in `(-π, π]`; phase `0` is a pure sine, `±π/2` a pure cosine |
| DC | the mean of the resampled trace over the window, `Σ wgt_i * y_i / divisions` (C's row `0`) |
| THD | `sqrt(Σ_{k≥2} (amplitude_k / amplitude_1)²)` — a **fraction** in the API, printed as a percentage in the text block (C's `thd = 100*sqrt(...)`, `src/frontend/fourier.c`) |

C prints the same phase in **degrees** (`atan2(cosine, sine)` scaled), so the opt-in comparison
converts once, explicitly. A zero fundamental amplitude leaves the THD undefined and is
`SpiceError::Numerical`, never a fabricated `inf`.

## Failure policy

A Fourier result that cannot be computed is an error, never a fabricated `NaN`/`0`/clamped
value. Exit statuses are `simulate`'s (`docs/port/CLI.md`): `NotYetPorted` → 3, everything else
→ 2.

| Input | Class |
| --- | --- |
| malformed card, missing or non-positive fundamental, non-whole or repeated `HARMONICS=`, a value that is a `{…}`-free non-literal, a missing vector, `all`, an AC-component spelling, an unknown parameter | `SpiceError::Parse` (2), positioned at the offending token |
| `NFREQS=`, `NPERIODS=`, `POLYDEGREE=`, `FOURGRIDSIZE=` (C reads them from interactive `set` variables), a `.four` card in a `.subckt` body | `SpiceError::NotYetPorted` (3) |
| a harmonic count beyond the port's resampling budget (`HARMONICS=101` with `MAX_HARMONICS = 100`), a card in a run that is not `.tran`, a plot without a `time` axis or with fewer than two points, a descending time axis, a run shorter than one period, a window with too few samples for the requested harmonics, a grid point on an unrepresented discontinuity, an unresolvable vector | `SpiceError::Unsupported` (2) |
| a non-finite axis, operand or result value, or a zero fundamental amplitude | `SpiceError::Numerical` (2) |

## Divergence from C

* **The grid is `4 * max(harmonics, 16)` subintervals per period, not a fixed 200.** C's
  `fourgridsize` defaults to 200 per period and is settable from the `set` command; the port
  scales the grid with the requested harmonic count and rejects `FOURGRIDSIZE=`. For a smooth
  trace both agree to the printed digits; for a discontinuous one the port's coarser grid
  integrates the step less accurately, in proportion to each harmonic's own size (measured on
  the shared square-wave deck: 2.0 % at harmonic 7 and 3.3 % at harmonic 9 against C's ~0.3 %).
* **Phase is radians.** C prints degrees.
* **THD is a fraction in the API** and a percentage in the text block; C only prints the
  percentage.
* **`nfreqs`, `nperiods`, `polydegree` and `fourgridsize` are not honoured.** C takes all four
  from interactive `set` variables and defaults them to `10, 1, 1, 200`; the port has one
  harmonic count (`HARMONICS=` or `9`) and rejects the others by name rather than silently
  using its own value.
* **A failing card fails the run.** C prints `Error: …` to `cp_err` and carries on with the rest
  of the batch job; the port returns an error, publishes no report and leaves an existing
  output destination untouched, because a transform the port cannot compute is not a successful
  run. C's failure messages are modelled in the port's wording: `no vectors loaded`, `fourier
  needs real time scale`, `bad fundamental freq`, `isn't real!`, `longer than time span`
  (`src/frontend/fourier.c:65-137`).
* **A grid point on an unrepresented discontinuity is an error.** C interpolates across it with
  a zero time span and reports nonsense; the port names the discontinuity.
* **Only `.four` is a card name.** C accepts the `tran` analysis word and `.four`; other
  spellings reach it through the same `ciprefix`-style matching as `.measure`, which the port
  does not reproduce.

## Not ported

`NFREQS=`, `NPERIODS=`, `POLYDEGREE=`, `FOURGRIDSIZE=`, `{…}` parameter values, `.four` inside a
`.subckt` body, `.four` on a non-transient run, `sp`/`sparam` parameters, and the interactive
`fourier` command. A full FFT library port and ngspice's other interpolation options are
explicit non-goals.

## Validation

* `crates/spice-analysis/src/fourier.rs` — unit tests over hand-built plots with analytic
  results: a pure sine and a known two-harmonic signal recover their amplitudes and phases, a DC
  offset is its own mean, the window/endpoint rules are pinned, and every failure class above has
  a test. Refusals cover a descending axis, a one-point plot, a short run, a harmonic count
  outside the budget, a non-finite sample and a zero fundamental.
* `crates/spice-netlist/tests/fourier_cards.rs` — the grammar, the positions, `DEFAULT_HARMONICS`
  and `MAX_HARMONICS`, the rejection classes, body-local cards, and the writer/semantic
  round-trip.
* `crates/spice-cli/tests/simulate.rs` — process tests over a real PULSE square wave: the block
  lists every card, the source's own harmonics match the analytic `2/(kπ)` series, the filtered
  output is scaled by the analytic lowpass `|H(f)|` and delayed by `-atan(2πfRC)`, a vector the
  output selection dropped is still transformed, a deck without `.four` prints exactly what it
  printed before and writes the same rawfile, and a failing card publishes nothing.
* `crates/spice-analysis/tests/c_four_reference.rs` — the opt-in `#[ignore]`d comparison against
  the local C binary (`NGSPICE_BIN`), over the same deck for both engines: DC, THD and the
  harmonic table, for a filtered voltage, the unfiltered square wave and a branch current. It
  needs no C for ordinary runs.
