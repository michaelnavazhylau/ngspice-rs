# Transmission lines

M10 slice 3 (#84) ports the **lossless transmission line `T`** (C
`src/spicelib/parser/inp2t.c`, `src/spicelib/devices/tra/`). The lossy lines
(`O`/LTRA, `Y`/TXL, `P`/CPL) remain `NotYetPorted`; LTRA is planned on the
same delay infrastructure (M10 slice 8). Code: `src/netlist/parser/tline.rs`
(grammar), `src/devices/tline.rs` (device), `src/devices/delay.rs` (delay
history, device breakpoints and step bounds shared with future lines). The
transient-driver design is the ADR in
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

* `backend=diffsol method=bdf`: a delay is not an index-one `E x' + A x =
  b(t)`; the backend refuses any circuit with a delay line.
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
