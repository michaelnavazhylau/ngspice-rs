# SPICE-compatible companion transient driver (#26)

Ordinary `.tran tstep tstop [tstart [tmax]]` runs the adaptive trapezoidal /
Gear-2 **companion-model** driver, `spice_analysis::companion_transient`
(`crates/spice-analysis/src/companion.rs`). It is a separate implementation from
the explicitly selected diffsol BDF backend (see
[DIFFSOL_FAER_IMPLEMENTATION.md](DIFFSOL_FAER_IMPLEMENTATION.md)); companion stamps
are never handed to diffsol and diffsol equations are never discretized again.
C references (read-only behaviour): `dctran.c`, `ckttrunc.c`, `cktterr.c`,
`niinteg.c`, `nicomcof.c`, `traninit.c`, `captrunc.c`, `indtrunc.c`.

## Selecting the backend and method

| Request | Result |
| --- | --- |
| no `backend=` (or `backend=companion`) | companion driver, trapezoidal rule |
| `method=trap` / `trapezoidal` | trapezoidal (order 2, backward Euler at startup and after breakpoints) |
| `method=gear` | Gear order 2 (same order policy; `maxord` 1 or 2) |
| `maxord=1` | backward Euler throughout (C: `CKTmaxOrder > 1` is required to raise the order) |
| `maxord=3..6`, `0`, non-integer | explicit `Unsupported` error (orders above 2 are not implemented) |
| `backend=diffsol method=bdf` | explicit diffsol BDF, unchanged |
| `backend=diffsol` without `method=bdf`, `method=bdf` without the backend, unknown backend/method, unknown or duplicate options | explicit errors |
| `uic`, device `ic=`, `.ic` / `.nodeset` cards | explicit errors (initial-condition semantics are GitHub #27) |

`.option method=`, `maxord=`, `reltol=`, `vntol=`, `abstol=`, `chgtol=` and
`trtol=` are forwarded by `RunConfig::request_for` as the request arguments
`method=`, `maxord=`, `rtol=`, `vntol=`, `abstol=`, `chgtol=`, `trtol=` unless
the request already states them (explicit request > deck > defaults). With
`backend=diffsol` a deck `method`/`maxord`/`chgtol`/`trtol` is an error, not
silently ignored. `maxsteps=` bounds accepted + rejected steps.

Defaults are ngspice's: `reltol` 1e-3, `vntol` 1e-6 V, `abstol` 1e-12 A,
`chgtol` 1e-14 C, `trtol` 7, `maxsteps` 1,000,000 (range 1 to 10,000,000). The
diffsol backend keeps its own tighter defaults (rtol 1e-7, vntol 1e-9 V).

## Algorithm

Per trial the driver loads every device into a fresh matrix, right-hand side and
`TrialState` (`Circuit::load`), solves, and then either **rejects** (the trial is
dropped; neither `StateHistory` nor `StepHistory` nor any device changes) or
**accepts** through `Circuit::accept_point` (accept hooks first, history commit
last, atomic) followed by `StepHistory::accept`. An accept-hook failure aborts
the run with an error; no partial plot is returned.

* **Initial point.** `A x = b(0-)`: the DC operating point with the sources at
  their left limit at `t = 0` (C evaluates the waveform in `MODETRANOP`), the
  same policy as the diffsol backend. Its charge/flux state, with zero
  derivative, fills the whole accepted history (C copies `CKTstate0` into
  `CKTstate1..3`). Floating capacitor nodes or ideal-source loops have no DC
  point and fail.
* **Step sizes** (`traninit.c`, `dctran.c`). `maxstep = tmax` if given, else
  `min(tstep, (tstop - tstart)/50)`; `delmin = 1e-11 maxstep`; first step
  `min(tstop/100, tstep)/10`, cut at the `t = 0` breakpoint to
  `0.1 min(tstop/50, gap to next breakpoint)` and divided by ten, at least
  `2 delmin`. `tstep` limits the step; it is not an output spacing.
* **Truncation error** (`CKTterr`, via `Coefficients::truncation_timestep`) for
  every device with `Device::truncation_slot()` (capacitors, inductors) from
  divided differences of the charge/flux over the trial and accepted points,
  with `reltol`, `abstol` (on the charge/flux derivative), `chgtol`, `trtol`.
  The next step is `min(2 dt, bound)`. A trial with a bound of at most `0.9 dt`
  is rejected and retried with the bound; the first step is never checked.
* **Order policy.** Order 1 (backward Euler) for the first step and the first
  step after every breakpoint; after an accepted order-1 step the order-2
  estimate is probed and order 2 is kept if it allows more than `1.05 dt`
  (C overwrites the step with the probe either way). Like C, the probe on the
  second step uses `maxstep` placeholders for the not-yet-existing older steps
  (`StepHistory::with_fill`). Non-convergence of a nonlinear trial divides the
  step by eight and returns to order 1.
* **Breakpoints.** Source corners and jumps are consumed lazily from
  `LinearSystem::breakpoints_in`, merged within `5e-5 maxstep` (`CKTminBreak`)
  and bracketed by `0` and `tstop`. A step is cut to land exactly on the next
  breakpoint (the step end is assigned the breakpoint time, not `t + dt`), or
  halved when the following step would otherwise be tiny. The step **ending** at a
  breakpoint evaluates the forcing with `Limit::Left`; later steps use
  `Limit::Right` at their (later) end time. History is *not* cleared at a
  breakpoint (C does not); only the order resets, so the post-breakpoint
  backward-Euler step re-derives the capacitor current/inductor voltage from
  continuous charge/flux. Source stamping is part of the device load
  (`IndependentSource::stamp` with `Forcing { limit, timing }`).
* **Minimum step and limits.** At or below `delmin` after a rejection the step is
  retried once at `delmin`, then the run fails with "timestep too small"
  (`dctran.c`). `t + dt <= t` is a "no progress" error. Accepted + rejected steps
  and pulled breakpoints are bounded by `maxsteps`.
* **Newton structure.** `load -> solve -> converged?` per trial. Linear circuits
  take one solve and then a matrix-free reload at the solution for the trial
  state (one factorization per trial). Nonlinear devices iterate up to 10 times
  with `reltol`, `vntol` on node rows and `abstol` on branch rows; nothing in the
  repository evaluates nonlinear physics yet (M4), the loop is exercised by test
  devices.

## Output policy

The plot holds **every accepted time point with `time >= tstart`**, exactly like
C's rawfile (`CKTdump` runs after each `CKTaccept`; the first row is the DC
point when `tstart = 0`). The `.tran` step is not an output grid and no samples
are interpolated, so no sample can span a source breakpoint, and every
breakpoint inside the run has a sample. C writes no second sample at a jump and
neither does this driver: the sample at a jump is the left limit and the next
sample is the first right-limit step. `xtask/src/tran.rs` consumes such plots
(a single sample at a breakpoint serves both limits, exact for the continuous
capacitor voltage/inductor current). Consumers that need a regular grid resample
inside an interval between two samples (never across a breakpoint), as the
comparator does. The simulation always starts at `t = 0`; `tstart` only
suppresses earlier rows.

## Measured accuracy (physical-error limits)

Analytic cases (`crates/spice-analysis/tests/companion_transient.rs`), unit step
into RC / RL (tau = 1 ms) and the series RLC (R = 10, L = 1 mH, C = 1 uF, zeta =
0.158), maximum step `h` set by `tmax` with truncation limiting disabled
(`rtol=1`):

| Case | method | h | max error | limit asserted |
| --- | --- | --- | --- | --- |
| RC v(out) | trap | 100 us / 50 / 25 | 2.6e-4 / 6.9e-5 / 1.8e-5 V | 4e-4 V, ratio 3.4 to 4.6 per halving |
| RC v(out) | gear | 100 us / 50 / 25 | 1.1e-3 / 2.8e-4 / 7.1e-5 V | 1.5e-3 V, same ratio band |
| RL i(l1) | trap, gear | 100 us / 50 / 25 | 2.6e-7 ... 1.8e-8 A (trap), 1.1e-6 ... 7.1e-8 A (gear) | 4e-7 A, 1.5e-6 A |
| RLC v(out) | trap | 4 us / 2 / 1 | 3.0e-3 / 7.6e-4 / 1.9e-4 V | 6e-3 V at h = 4 us |
| RLC v(out) | gear | 4 us / 2 / 1 | 1.2e-2 / 3.0e-3 / 7.7e-4 V | 2e-2 V at h = 4 us |

Both rules show the expected order 2 (error / 4 per halving of `h`). Under
truncation control only (`tmax = 2 ms`) tightening `reltol` from 1e-3 to 1e-5 to
1e-7 reduces the RC step error by more than a factor of five at each stage. A
PULSE-driven RC (corners at 1 ms, 1.2 ms, 3.2 ms, 3.3 ms and the next periods)
lands on every corner within 1e-15 s and keeps the order-2 trend; the
backward-Euler restart step after each corner puts a floor under the last
halving of Gear-2.

Against C (`NGSPICE_BIN`, opt-in `c_companion_reference.rs`, same deck text, both
resampled at common times with 1 us / 1 pA absolute floors and
`1e-3 |C|` relative): the PULSE and PWL RC decks, the series RLC deck and the
committed `rc_transient.cir` all agree far inside the bound. The step sequence of the
port reproduces C's (identical point counts for the RC and `rc_transient`
decks; first steps equal to 1e-15 relative), because the controller follows
`dctran.c`; this is a measured outcome, not a requirement. Tolerances are
`xtask::compare::TRAN` (ngspice's own `reltol`, `vntol`, `abstol`); no tolerance
was loosened and no golden was recaptured.

`cargo xtask golden verify` registers `rc_transient` with `compare::TRAN` against
the committed `conformance/golden/rc_transient.raw` (11 interior instants plus
both limits of the single in-run breakpoint; worst error 0.000 of the bound).

## Limits

* Linear R/C/L/V/I only; nonlinear charge and devices arrive with M4.
* Orders above 2, `.ic`, `.nodeset`, `uic` and instance `ic=` are rejected.
* No predictor (`PREDICTOR` is optional in C); the previous solution seeds Newton.
* No `gmin`/source stepping: a floating or source-looped DC bias is an error.
* General DAE structure is not analysed (no index check as in the diffsol
  backend); a singular trial matrix is an explicit numerical error.
* Hooking IC initialization (#27): replace the left-limit bias solve in
  `Driver::run` (`self.system.a.solve(...)`) and `Driver::accept_initial` with a
  routine that produces the initial `x` and a consistent charge/flux trial state;
  everything after that point (history fill, step control) needs no change.
