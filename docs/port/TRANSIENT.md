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
| `uic`, `.ic`, instance `ic=` with `backend=diffsol` | explicit `Unsupported` errors (the BDF backend has no IC formulation; `.nodeset` is validated and otherwise a no-op) |
| `uic`, `.ic`, `.nodeset`, instance `ic=` on the companion driver | implemented, see [Initial conditions](#initial-conditions-27) |

`.option method=`, `maxord=`, `reltol=`, `vntol=`, `abstol=`, `chgtol=` and
`trtol=` are forwarded by `RunConfig::request_for` as the request arguments
`method=`, `maxord=`, `rtol=`, `vntol=`, `abstol=`, `chgtol=`, `trtol=` unless
the request already states them (explicit request > deck > defaults). With
`backend=diffsol` a deck `method`/`maxord`/`chgtol`/`trtol` is an error, not
silently ignored. `maxsteps=` bounds accepted + rejected steps.
`.option itl4` bounds the Newton iterations of each nonlinear trial: C's
nominal `CKTtranMaxIter` default is 10, but `NIiter()` (`niiter.c`) raises any
limit below 100 to 100, so the effective default is 100 and the deck value is
forwarded as request `tranmaxiter=max(itl4, 100)` (the request key itself is a
literal `1..=10000` port knob),
`.option xmu` (request `xmu=`, `0..=0.5`, default 0.5) is the trapezoidal
weighting of `nicomcof.c`, and `itl1`/`itl2`/`srcsteps`/`gminsteps`/`gminfactor`
(requests `maxiter=`, `stagemaxiter=` etc.) configure the nonlinear initial bias (#110). All of
them are rejected with `backend=diffsol`.

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

* **Initial point.** Without `uic`: `A x = b(0-)`, the DC operating point with
  the sources at their left limit at `t = 0` (C evaluates the waveform in
  `MODETRANOP`), the same policy as the diffsol backend, with `.ic` node
  voltages imposed (below). With `uic`: no solve, see below. The charge/flux
  state, with zero derivative, fills the whole accepted history (C copies
  `CKTstate0` into `CKTstate1..3`). Floating capacitor nodes or ideal-source
  loops have no DC point and fail unless an `.ic` fixes the floating node.
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

## Initial conditions (#27)

C references (all behaviour below was read from the sources and re-checked with
the reference binary): `inppas3.c`/`cktsetnp.c` (storing `.ic`/`.nodeset`),
`cktic.c` (`CKTic`), `cktload.c` (constraint stamping), `niiter.c`
(`MODETRANOP`/`MODEUIC` shortcut), `dctran.c`, `capload.c`, `capgetic.c`,
`indload.c`. Code: `crates/spice-analysis/src/initial.rs` and
`Driver::initialize` in `companion.rs`. Tests: `tests/initial_conditions.rs`
(analytic) and opt-in `tests/c_initial_conditions.rs` (live C).

| Item | C behaviour | This port |
| --- | --- | --- |
| duplicate `.ic`/`.nodeset` for a node | each entry overwrites the node field: last wins | same (deck order, across cards) |
| unknown node | warning "IC on non-existent node ... ignored" (any analysis) | **error** with the entry location, in every analysis and backend |
| `.ic`, no `uic` | enforced in every iteration of the `.tran` initial bias only (`cktload.c`, `MODETRANOP`, not `MODEUIC`); row replaced by `v = ic`, or by a `1e10` conductance when the row holds a branch-current entry | node rows replaced by `x[row] = ic` **exactly** (the KCL residual is the current of the hidden constraint), for every row. Nodes on ideal sources/inductors (DC shorts) are resolved structurally, below |
| `.ic` in `.op`, `.dc`, `.ac` | ignored (`MODETRANOP` only) | accepted, ignored, nodes validated |
| `.nodeset`, no `uic` | stamped only in `MODEINITJCT`/`MODEINITFIX` iterations, then released; a convergence hint | linear circuits have one solution, so it is validated and has no effect (tested bit-identical); `.op`, `.dc`, `.ac`, both backends |
| instance `ic=` (C, L), no `uic` | ignored (`capload.c`/`indload.c` use it only for `MODEUIC`) | ignored |
| `uic` | `NIiter` returns after one `CKTload`, no solve; `CKTic` copies `.nodeset` then `.ic` into the node vector; `CAPgetic` takes an unset capacitor `ic` from those node values (instance `ic=` always wins); inductor current = `ic=` else 0; no `t = 0` row is written; a breakpoint at the `.tran` step is added (`CKTsetBreak(CKTstep)`) | same, including `.nodeset` acting as a node IC and the missing `t = 0` row and the step breakpoint; the first row is the first accepted time |

**Without `uic`.** The `.ic` constraints replace the KCL rows of the constrained
nodes in `A x = b(0-)`; the result is the `t = 0` row, and the charge/flux state
is `C v`, `L i` of that point. Then the constraint is released: the run is an
ordinary transient (verified against `1 - 0.75 exp(-t/tau)`, max error
2.3e-6 V at 10 us steps, and against C at 1.6e-9 of the C tolerance bound).
Nodes whose voltage is already fixed by ideal voltage sources and inductors (DC
shorts) chained to ground, or to another constrained node, are solved
structurally (petgraph spanning forest with `UnionFind` membership of the `v+ - v- = value` relations): an `.ic`
that agrees within `reltol`/`vntol` is dropped (the source wins; C returns a
wrong `i(v1)` of 0 at `t = 0` for this case because of its `1e10` hack, this port
returns the exact branch current), one that disagrees, or two entries on rigidly
tied nodes, is an explicit error instead of C's meaningless `1e10` compromise
(`i(v1) = -1e10`). A node with only capacitors (no DC point) becomes solvable
through its `.ic`.

**With `uic`.** Capacitor charge is `C ic` (instance `ic=`, else the difference of
the `.nodeset`/`.ic` node values, else 0), inductor flux is `L ic` (default 0
current; positive from the first to the second terminal). `x` is only the Newton
guess; the first timepoint is solved with backward Euler from that state and
sources at their right limit at `t = 0`. C never checks consistency; this port
does, **exactly**, before any accept hook or plot row exists
(`initial::check_impulse_free`): it forms the instantaneous problem (capacitors
become voltage constraints with free current, inductors fixed currents with free
voltage, sources at the right limit) and requires a solution. Redundant but
consistent relations (a capacitor whose `ic` equals the source across it, an
inductor carrying the series current source's current) are accepted;
contradictions (capacitor across an ideal source with another `ic`, a capacitor
loop whose voltages break KVL, an inductor current against a series current
source, a step source whose right limit at `t = 0` differs from the capacitor
`ic`) are errors naming the element; anything else singular or non-square is
reported as ill-posed. Failed initialization returns before any state exists
(tested with an accept-hook probe: zero hooks called).

Numerical consequences and divergences from C: no `1e10` scaling anywhere; `i(v1)`
at `t = 0` is exact where C's artifact differs (compared after `t = 0` in the
opt-in test); `.ic` entries contradicting a source and impulsive `uic` states are
errors where C produces garbage or a first-step spike; unknown nodes are errors.
Coupled inductors (K, #80) start from the coupled fluxes `L ic + sum(M ic_k)`;
see [MUTUAL_INDUCTANCE.md](MUTUAL_INDUCTANCE.md). Not covered: nonlinear device initial conditions
(`off`/`ic=` of diodes/transistors), `.ic` inside subcircuits and `.nodeset all=`
(`NotYetPorted` in the parser), `.op`-only `.ic` use.

Measured against analytic solutions (trap, `tstep` = max step): `uic` RC discharge
from `ic=2`, 10 us step: 5.9e-6 V; `uic` RL (`ic=0.5 A`), 10 us: 1.2e-6 A; `uic`
series RLC with inductor and capacitor `ic`, 0.5 us: 1.0e-4 V, 3.3e-6 A. Against C
(same decks, common 1e-3 relative / 1 uV / 1 pA bound): all 11 opt-in cases agree
to better than 3.1e-9 of the bound, with identical point counts (the step
sequences coincide again, including the `uic` step breakpoint).

## Source functions (#94, #95)

Independent V and I sources accept every standard ngspice transient function
except the noise/random/external ones. Syntax lives in
`crates/spice-netlist/src/parser/waveform.rs`; runtime semantics in
`crates/spice-devices/src/functions.rs` (SIN/EXP/SFFM/AM, PWL `td=`/`r=`) and
`pulse.rs` (PULSE count), following `vsrcload.c`/`isrcload.c`,
`vsrcacct.c`/`isrcacct.c` and `vsrcpar.c`/`isrcpar.c`.

| Form | Fields (C order) | Defaults resolved from `.tran` |
| --- | --- | --- |
| `SIN`/`SINE` | `VO VA [FREQ [TD [THETA [PHASE]]]]` | FREQ omitted or 0: `1/tstop`; others 0 |
| `EXP` | `V1 V2 [TD1 [TAU1 [TD2 [TAU2]]]]` | TD1, TAU1, TAU2 omitted or 0: `tstep`; TD2 omitted or 0: `TD1 + tstep` |
| `SFFM` | `VO VA [FC [MDI [FM [TD [PHASEM [PHASEC]]]]]]` | FC omitted: `5/tstop`; MDI omitted: 90, then limited to `[0, FC/FM]`; FM omitted or 0: `500/tstop` |
| `AM` | `VO VMO [VMA [FM [FC [TD [PHASEM [PHASEC]]]]]]` | VMA omitted: 1; FM omitted: `5/tstop`; FC omitted: `500/tstop` |
| `PULSE` 8th field | `NP` | positive: only `NP` periods, then V1; zero/negative/omitted: unlimited |
| PWL `td=` / `r=` | ordered scalar setters | `td` shifts every knot; `r` (a knot time below the last) repeats `[r, t_last]`; `r < -0.5` disables repetition |

Phases are degrees. Before its delay a SIN holds `VO + VA sin(PHASE)` and an
EXP holds V1, but SFFM and AM hold **zero** (C returns 0 for `time <= TD`), so
they jump at their delay unless `VO + VA sin(...)` vanishes there. A repeating
PWL jumps at each repetition boundary when `v(r) != v(t_last)`. Every jump has
distinct left/right limits (`Limit`), including every repeated-PWL boundary
(left `v(t_last)`, right `v(r)`, also a few ulps either side of the port's own
breakpoint times), so the step ending at a boundary integrates toward
`v(t_last)` in the companion driver and the diffsol segments see the full ramp.
This deliberately differs from C, which evaluates a boundary instant once: its
next PWL breakpoint is accumulated from the previous landing time
(`VSRCaccept`), and depending on which side of `t_last` that time's fold
rounds, `vsrcload.c` loads `v(t_last)` or `v(r)` there. Bit-identical landing
would require reproducing C's stateful breakpoint chain and its
`CKTtime += CKTdelta` landing arithmetic, which the port does not do. The
effect is bounded by one step of the jump: with tau = 0.1 ms and 7-10 us steps
the worst RC `v(out)` difference from C is 0.024-0.036 V for a 1 V sawtooth
(`pwl(0 0 1m 1) r=0`, `pwl(0 0 0.3m 1) r=0`, and a delayed partial repeat).
OP/DC analyses without an explicit DC
value use C's time-zero value (`Waveform::time_zero`); the transient initial
point uses the left limit at `t = 0`, as C's `MODETRANOP` load does (an explicit
`dc` value only affects OP/DC, as in C).

Breakpoints are lazy (`Waveform::breakpoints_in`). PULSE/PWL corners are the
ones `VSRCaccept` sets, including every repeated PWL knot; a count-limited
PULSE stops after `TD + NP*PER` except for C's final request (the next corner)
and, for a fractional count, the jump where the train is cut. C sets **no**
breakpoints for SIN/EXP/SFFM/AM; the port deliberately lands on SIN/SFFM/AM
`TD` and EXP `TD1`/`TD2` so it never integrates across a slope corner or the
SFFM/AM delay jump. Measured on RC decks (tau = 0.1 ms) without marker
sources, this changes the worst `v(out)` difference from C to 2e-5 V (SIN with
`TD`), 8e-5 V (EXP), 1.1e-3 V (SFFM/AM delay jump), 9e-3 V (EXP with
`TD2 < TD1`, a jump at `TD1`) and 1e-3 V (PULSE with a fractional count); the
opt-in `unmarked_decks_diverge_from_c_only_within_documented_bounds` keeps
these bounded. The diffsol BDF backend rejects SIN/EXP/SFFM/AM (its segments
interpolate forcing linearly between breakpoints); PULSE counts and PWL
`td=`/`r=` (including discontinuous repeats, segmented at every boundary) are
piecewise linear and run there.

Deliberate differences, all explicit errors rather than approximations:
negative delays (SIN/SFFM/AM `TD`, EXP `TD1`/`TD2`, as for PULSE `TD`); `r=`
that matches no knot or is not below the last knot (C's `E_PARMVAL`, checked
for every `r=` setter in order, so an invalid earlier one fails even when a
later one is valid); `r=`
before any PWL, a waveform setter after `r=` (C would keep a stale repeat
index) and `td=`/`r=` without a final PWL (C silently ignores them); more
fields than C reads. SFFM's MDI limit is silent (C warns once). ngspice's `xs`
compatibility mode, where the PULSE eighth field is a phase, is not modelled.
TRNOISE, TRRANDOM, EXTERNAL, PWL `file=` and expression-valued fields remain
`NotYetPorted`.

Verification: analytic unit tests per form and default; deck-level tests
(`crates/spice-devices/tests/waveforms.rs`,
`crates/spice-analysis/tests/source_functions.rs`, including an analytic EXP RC
response under trap and Gear-2 and a `.four` of a SIN-driven RC: fundamental
gain/phase of the low-pass within 1e-3 and THD below 0.1 %). C goldens
`rc_sin_tran`, `rc_exp_tran`, `rc_sffm_am_tran` (AM as a current source),
`rc_pwl_repeat_tran` (continuous `r=` repeat) and `rc_pulse_count_tran`
verify under `compare::TRAN` with worst error 0.000 of the bound. There is no
golden for a discontinuous `r=` repeat because of the boundary difference
above; `source_functions.rs` checks one against the analytic RC response under
both backends instead. Because C sets no SIN/EXP breakpoints, the
SIN and EXP decks carry a constant PWL marker source whose knots make C land on
the corners too; otherwise the comparison would interpolate C's plot across a
slope corner. Opt-in `c_source_functions.rs` compares 22 V/I sources (all forms
and defaults) with C's node voltages at all of C's timepoints (1e-9 relative),
their `.op` values, the `.four` THD and harmonics of a SIN-driven diode
clipper, and the documented divergences on unmarked decks.

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
* Orders above 2 are rejected; `.ic`/`uic`/instance `ic=` exist only on this
  driver (explicit errors with `backend=diffsol`).
* No predictor (`PREDICTOR` is optional in C); the previous solution seeds Newton.
* No `gmin`/source stepping: a floating or source-looped DC bias is an error.
* General DAE structure is not analysed (no index check as in the diffsol
  backend); a singular trial matrix is an explicit numerical error.
* Companion inductor rows are row-equilibrated before the solve
  (`companion.rs::equilibrated`); without it a nonzero initial inductor current
  with `L/dt` of 1e5 or more tripped the solver's backward-residual check.
