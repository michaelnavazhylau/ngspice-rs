# Transmission lines

M10 slice 3 (#84) ports the **lossless transmission line `T`** (C
`src/spicelib/parser/inp2t.c`, `src/spicelib/devices/tra/`) and M10 slice 8
(#85 part 2) the **lossy line `O`** with `ltra` models (C
`src/spicelib/parser/inp2o.c`, `src/spicelib/devices/ltra/`, see
[below](#lossy-line-o-ltra-85)). `Y`/TXL and `P`/CPL remain `NotYetPorted`.
Code: `src/netlist/parser/{tline,ltra}.rs` (grammars),
`src/devices/{tline,ltra}.rs` and `src/devices/ltra/convolution.rs`
(devices), `src/devices/delay.rs` (delay history, device breakpoints and step
bounds shared by both). The transient-driver design is the ADR in
[TRANSIENT.md](TRANSIENT.md#device-driven-breakpoints-and-delay-history-adr-84).

## Card

```text
Tname p1 n1 p2 n2 z0=<ohms> [td=<s> | f=<Hz> [nl=<length>]]
      [ic=v1[,i1[,v2[,i2]]]] [v1= i1= v2= i2=] [rel=<r>] [abs=<a>]
```

| Item | C behaviour | This port |
| --- | --- | --- |
| setters | `TRApTable`: `z0` (alias `zo`), `td`, `f`, `nl`, `v1`, `i1`, `v2`, `i2`, `ic` (`IF_REALVEC`, fallthrough `v1 i1 v2 i2`), `rel`, `abs`; applied in written order, last wins; `=` optional | same, ordered in the AST |
| `z0` | required (`TRAsetup`: "transmission line z0 must be given") | same; `z0 <= 0` is an error (C divides by it) |
| delay | `td` given wins, else `td = nl / f` with `nl = 0.25`, `f = 1 GHz` (`trasetup.c`, `tratemp.c`) | same; `td <= 0` (or `f`/`nl <= 0` without `td`) is an error (C's history then has coincident times and its load skips the source) |
| `rel`, `abs` | slope-change tolerances of `traacct.c`/`tratrunc.c`, default 1 and 1 | same; negative values are errors |
| leading value | `INPdevParse` reads and `INP2T` ignores it | refused (`Unsupported`) |
| unknown word | "unknown parameter" | parse error |
| `@t1[z0]`, `td`, `nl`, `f`, `v1`..`i2`, `rel`, `abs` | `TRAask` | scalar observations |

## Model and unknowns

Branin's method of characteristics, exactly as `traload.c` stamps it. Each
port is `Z0` in series with a voltage source driven by the wave that left the
other port `TD` earlier:

```text
v(int1) - v(n1) = [v(p2) - v(n2) + Z0 i2](t - TD)
v(int2) - v(n2) = [v(p1) - v(n1) + Z0 i1](t - TD)
```

`i1`/`i2` are the currents **into** the positive terminal of port 1/2. As in C
(`trasetup.c` creates them with `CKTmkVolt`, not as branch currents), the two
currents and the two internal nodes are node unknowns named `<name>#i1`,
`<name>#i2`, `<name>#int1`, `<name>#int2` (`NodeKind::Internal`), allocated
with the device's other nodes, so plots carry `v(t1#i1)` (amperes, typed
`voltage` exactly as ngspice writes and saves them; `outitf.c` does not omit
these names). `golden verify` keeps them in the comparison.

| Analysis | Stamp | C |
| --- | --- | --- |
| `.op`, `.dc`, transient bias | zero delay: `v(int1) - v(n1) = v(p2) - v(n2) + (1 - gmin) Z0 i2` and symmetrically | `traload.c` `MODEDC` |
| `.ac`, `.sp` | cross terms times `exp(-j omega TD)`, reassembled at every frequency (`Device::small_signal_depends_on_frequency`; the operating point is not re-solved) | `traacld.c` |
| transient (companion) | static part plus `input1/2` from the delay history, below | `traload.c`, `traacct.c`, `tratrunc.c` |

AC: the port's small-signal pencil is `A + j omega E`, which cannot hold
`exp(-j omega TD)`. At each frequency the line puts the real part of each
complex coefficient in `A` and the imaginary part divided by `omega` in `E`,
which reproduces C's coefficient at that frequency. At frequency zero (the
operating-point assembly) it stamps the DC form, which differs from the AC
limit only by C's `(1 - gmin)` factor.

## Transient

* **History.** At `t = 0` the history is three equal samples at `-2 TD`,
  `-TD` and `0` of the two outgoing waves `[v(p2) - v(n2) + Z0 i2, v(p1) -
  v(n1) + Z0 i1]` of the operating point (`MODEINITTRAN`). Each accepted point
  first drops samples no later interpolation can reach (keeping two before
  `t - TD`), then appends its waves unless it is within C's `CKTminBreak` of
  the previous sample (`traacct.c`). Trials only read it.
* **Load.** `input1/2` are the waves at `t - TD`, interpolated quadratically
  through the samples `i-2, i-1, i`, where `i` is the first sample from the
  third on later than `t - TD` (or the last one), with C's operation order.
* **Breakpoints.** After appending, if either wave's slope over the last step
  differs from the previous one by at least `rel * max(|d1|, |d2|) + abs`
  (`CKTdeltaOld[0]` and `[1]`), the line requests a breakpoint at the
  *previous* sample plus `TD`, where that corner arrives at the other port.
  With the defaults (`rel = abs = 1`) this fires only where a slope reverses;
  smaller values make the line land on every delayed corner.
* **Step bound.** For a converged trial, the same test between the trial's
  waves and the last two samples (with C's `CKTdeltaOld[1]` and `[2]`) bounds
  the step to `last sample + TD - t` (`tratrunc.c`).

Because the driver follows `dctran.c` and the line follows these three
routines, the port takes C's timepoints on the fixtures and opt-in decks,
including the breakpoint-landing ones (identical point counts; values within
rounding). The exception is listed below (breakpoints closer than the port's
merge threshold).

## Unsupported (explicit errors)

* `backend=diffsol`: the removed BDF backend is an explicit error for every
  circuit (a delay is not an index-one `E x' + A x = b(t)` either).
* `.tran ... uic` with a line (`NotYetPorted`, `traload.c` `MODEUIC`): the
  instance `ic=`/`v1 i1 v2 i2` values are parsed and reported but only C's
  `uic` start uses them, and the port's impulse-free check has no line
  formulation yet. Without `uic` C ignores them too.
* `.pz`: C has no TRA pole-zero load (`DEVpzLoad` is `NULL` in `trainit.c`,
  there is no `trapzld.c`), so ngspice silently leaves the line out of the
  pole-zero matrix; `exp(-s TD)` is not a polynomial pencil either. Refused.
* `.noise`, `.disto`, `.sens`: C has no TRA routines (`DEVnoise`, `DEVdisto`,
  `DEVsen*` are `NULL`); refused as `NotYetPorted` rather than treated as
  noiseless/linear/insensitive.
* XSPICE-only step behaviour: the port merges breakpoints within its own
  `5e-5 CKTmaxStep` (the reference binary uses `10 CKTdelmin`); decks whose
  line breakpoints fall closer than that to another breakpoint (for example two
  lines with `rel=1e-3` ringing through a common node) can take different,
  equally accurate steps from C.
* A `td` much shorter than the step forces steps of about `TD`
  (`tratrunc.c`): on the one deck tried (`td = 10 ps`, `.tran 0.2n 40n`) the
  port finished in about 2 s (33k points) while C did not finish within
  2 minutes; the two were therefore not compared.

## Verification

* Analytic (`tests/tline.rs`): matched line (delayed half-amplitude copy, no
  reflection), open end (doubling at the far end and at the near end after
  `2 TD`), shorted end (cancellation after `2 TD`, current doubling),
  mismatched source and load (plateaus `(2/3)(1.6) sum (-0.2)^k` and the
  resistive limit), DC wire, AC input impedance
  `Z0 (ZL + j Z0 tan(omega TD)) / (Z0 + j ZL tan(omega TD))` with `f`/`nl`,
  setter order and defaults, every error above. Measured errors are rounding
  level (about 1e-15 V); the asserted bounds are 1e-9 V (1e-12 for currents),
  well below `vntol`.
* Infrastructure (`tests/delay_lines.rs`, `companion.rs` unit tests): exact
  landing on device breakpoints, history equal to the accepted points under
  forced rejections, atomic failure, merge rules, backend refusals.
* C goldens (`golden verify`): `m10_tline_tran` (matched and mismatched
  lines), `m10_tline_pulse` (breakpoint propagation through two lines and a
  buffer) under `compare::TRAN` with the echo breakpoints declared per deck
  (`Gate::Echoes`), and `m10_tline_ac` (quarter-wave transformer and shorted
  stub) under `compare::AC`. All report worst error 0.000 of the bound.
* Opt-in live C (`tests/c_tline_reference.rs`): default `rel`/`abs` and an
  incommensurate delay (same rows, `1e-9 |C| + 1e-12`), a line into a diode
  (same rows, C's own `reltol`), AC with `f`/`nl` and a shorted stub inside a
  subcircuit, `.op` and `.sp`.

## Lossy line `O` (LTRA, #85)

```text
Oname p1 n1 p2 n2 model [v1= i1= v2= i2=] [ic=v1[,i1[,v2[,i2]]]]
.model model ltra [r=] [l=] [g=] [c=] len= [rel=] [abs=] [compactrel=]
       [compactabs=] [steplimit|nosteplimit] [lininterp|quadinterp|mixedinterp]
       [truncdontcut] [truncnr] [nocontrol] [ltra]
```

| Item | C behaviour | This port |
| --- | --- | --- |
| model name | required (`INP2O`; a missing model ends in "length must be given") | parse error; a non-`ltra` model is "incorrect model type" |
| instance setters | `LTRApTable`: `v1 i1 v2 i2` (`IF_REAL`), `ic` (`IF_REALVEC`, fallthrough `v1 i1 v2 i2`) | same, ordered; used only by `uic` (refused) |
| leading value | ignored | refused (`Unsupported`) |
| `r l g c len` | per-metre values, 0 by default; `len` required (`ltraset.c`) | same; negative values and `len <= 0` are errors |
| `rel`, `abs` | breakpoint slope tolerances, default 1 and 1 (output-only in the table but settable through `.model`) | same, nonnegative |
| `compactrel`, `compactabs` | straight-line tolerances (`LTRAstLineReltol/Abstol`), 0 means `reltol`/`abstol` | same; used by the RLC `LTRAmaxSafeStep` |
| flags | `IF_FLAG`, written order: `steplimit` (default) / `nosteplimit`, `quadinterp` (default) / `lininterp` / `mixedinterp`, `truncdontcut`; `ltra`, `nocontrol`, `truncnr` change nothing reachable | same |
| `level`, unknown setters, instance defaults on the card | warning / ignored / instance defaults | errors |
| `.options trytocompact` | linear interpolation and history compaction | refused by the options layer (unimplemented option) |

### Cases (`ltraset.c`)

| Case | Condition | DC | AC | Transient |
| --- | --- | --- | --- | --- |
| LC | `R = G = 0` | `i1 + i2 = 0`, `v(p1) - v(p2) = 0` | exact | delayed waves (`Y0 v + i` at `t - TD`) |
| RLC | `G = 0`, `R, L, C > 0` | `v(p1) - v(p2) = R len i1` | exact | attenuated delayed waves plus `h1'`, `h2`, `h3'` convolutions (Bessel responses) |
| RC | `G = L = 0` | as RLC | exact | `h1'`, `h2`, `h3'` convolutions (`erfc` responses), no delay |
| RG | `L = C = 0` | exact `cosh`/`sinh` two-port (with C's `1 + gmin` factors) | refused (below) | the DC two-port at every point |
| RL, `G` with `L` or `C`, fewer than two nonzero | errors, as in C | | | |

The DC rows of the LC/RLC/RC cases are C's: they relate only the positive
terminals (`v(p1) - v(p2) = R len i1`), which matches a line whose negative
terminals are both ground and is reproduced as is (a `.dc` check with a
floating negative terminal matches C to rounding).

The two unknowns are C's `CKTmkVolt` nodes `<name>#i1` and `<name>#i2`
(currents into the positive terminals, typed `voltage` in rawfiles); there
are no internal voltage nodes. The AC load is `ltraacld.c`'s exact
`Y0(s) V1 - I1 = exp(-lambda(s) len) (Y0(s) V2 + I2)`, assembled per
frequency like the lossless line (`A += Re`, `E += Im / omega`).

### Transient

* **History** (`ltraacct.c`, `dctran.c` `CKTtimePoints`): the port voltages
  and currents of every accepted point, starting with the operating point at
  `t = 0`, kept in the device's `DelayHistory` (width 4) and never pruned:
  every load convolves the whole past (`O(N)` per load, `O(N^2)` per run, as
  in C). Trials only read it.
* **Load** (`ltraload.c`, `ltramisc.c`): at each trial time the line computes
  `LTRArlcCoeffsSetup`/`LTRArcCoeffsSetup` over the accepted times (C keeps
  them per model for the time point; the port recomputes the same values per
  load), stamps the coefficient of the unknown sample, and puts the
  convolution of the history minus the operating point, the operating-point
  integral terms (`LTRAintH*`) and the attenuated delayed waves into the
  right-hand side. The delayed values use C's interpolation choice and its
  operation order. C's `LTRAchopReltol` has no deck setter (0), so no
  coefficient is chopped unless it is exactly zero.
* **Breakpoints** (`ltraacct.c`): after recording a point, if either
  attenuated wave `(v + Z0 i) exp(-beta TD)` changes slope by at least
  `rel max(|d1|, |d2|) + abs` and is not steady (C's `CHECK`: range of the
  last three values at least `50 (reltol (sum)/3 + abstol)`), the line asks
  for a breakpoint at the previous time plus `TD`. Unlike `traacct.c`, C does
  not test that this lies ahead: a request in the past is `CKTsetBreak`'s
  "breakpoint in the past" error, which the port reproduces as an error
  (unreachable with the default `steplimit`, which keeps steps at most `TD`).
* **Step bound** (`ltratrun.c`): LC/RLC steps at most `TD` (`steplimit`), or
  under `nosteplimit` when the slope test fires for the converged trial;
  RLC lines also at most `LTRAmaxSafeStep` (the bisection of `ltratemp.c`,
  with C's duplicated `h2` straight-line test) unless `truncdontcut`. C's
  convolution truncation-error control (`LTRAlteCalculate`, `truncnr`) needs
  `LTRAlteConType` other than `nocontrol`, which no deck can set.

The port therefore takes C's timepoints on every deck tried. LC, RC and RG
lines agree with C to rounding. RLC lines agree to about `1e-7` relative:
`ltramisc.c` branches on exact time equalities (`curtime - t_i == TD` picks
the auxiliary index, `t == t_k` the interpolation shortcuts), and at a step
that lands on a delayed source corner the port's exact breakpoint landing and
C's `t + (bp - t)` can take different branches.

### Unsupported (explicit errors)

* `backend=diffsol method=bdf` (as for `T`), `.tran ... uic`
  (`NotYetPorted`, `ltraload.c` `MODEINITTRAN | MODEUIC`).
* `.pz`: C has no LTRA pole-zero load and `PZinit`'s transmission-line check
  finds only TRA lines, so ngspice silently drops the line. Refused.
* `.noise`, `.disto`, `.sens`: no C routines; refused.
* **AC with an RG line**: `LTRAacLoad` calls the DC load of *every* LTRA
  model and returns when it meets an RG model, so in C any other lossy line in
  the circuit loses or doubles its AC load (an LC line beside an RG line
  comes out two orders of magnitude off; checked live in
  `c_ltra_reference`). An RG line alone is right in C, but the device cannot
  see the other lines, so the port refuses RG lines in AC/`.sp`.
* Nonfinite convolution terms: C's Bessel approximations overflow once
  `beta t` exceeds about 700 and C continues with NaN; the port stops with an
  error.
* No instance observations: C's `@o1[v1]` etc. return the operating point
  it copied into them during a transient, not the card values.

### Verification

* Analytic (`tests/ltra.rs`): a matched LC line (delayed half-amplitude copy,
  steps bounded by `TD`), the LC case against the lossless `T` line and the
  exact ringing of a mismatched line, an RC line against the open-ended
  diffusion series (error 3.2e-3 V just after the step, the same as C's own),
  an RLC line's `exp(-beta TD)` first arrival and DC limit, the DC two-ports
  (RG `cosh`/`sinh`), AC against `1/cosh(gamma len)` for LC/RLC/RC (1e-9),
  flag order, every refusal, the grammar and the deck writer.
* C goldens (`golden verify`, `compare::TRAN`/`compare::AC`):
  `m10_ltra_tran` (LC ringing, RLC with `mixedinterp`, RG; worst 0.024 of the
  bound, from the RLC branches above), `m10_ltra_rc_tran` (diffusion line and
  a subcircuit copy with `lininterp nosteplimit`; 0.000) and `m10_ltra_ac`
  (quarter- and half-wave LC, RLC, RC).
* Opt-in live C (`tests/c_ltra_reference.rs`): one LC/RLC/RC/RG deck under
  eight flag sets (same rows, `1e-5 |C| + 1e-9`), a line into a diode (C's
  `reltol`), AC of LC/RLC/RC lines in a subcircuit, `.op`, `.dc` with an RG
  line and floating negative terminals, `.sp`, and the RG AC defect.
