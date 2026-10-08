# Voltage- and current-controlled switches S/W (#81)

**Implemented: S (`sw` model) and W (`csw` model) switches for `.op`, `.dc`,
`.ac` and the companion `.tran` (trapezoidal/Gear-2), with C's hysteresis
rules, `on`/`off` initial flags, Newton-phase state handling, accepted switch
state and `swtrunc.c` step control.** The explicit diffsol BDF backend rejects
switches (they have no immutable linear form), and nonlinear `.ic`/`uic`
initialization stays unsupported for them as for every nonlinear device.

C references (read-only): `src/spicelib/parser/inp2s.c`, `inp2w.c`;
`src/spicelib/devices/sw/` (`sw.c`, `swsetup.c`, `swmparam.c`, `swparam.c`,
`swload.c`, `swacload.c`, `swtrunc.c`), `src/spicelib/devices/csw/` (`csw.c`,
`cswsetup.c`, `cswmpar.c`, `cswparam.c`, `cswload.c`, `cswacld.c`,
`cswtrunc.c`), `src/maths/ni/niiter.c` (`MODEINITF` phases),
`src/spicelib/analysis/dctrcurv.c`, `dctran.c`, `acan.c`. The Rust side is
`crates/spice-netlist/src/parser/switch.rs` (grammar),
`crates/spice-devices/src/switch.rs` (device), the Newton phases in
`crates/spice-devices/src/state.rs` and `crates/spice-analysis/src/newton.rs`.

## Syntax

```text
Sname n+ n- nc+ nc- model [on|off]...
Wname n+ n- vname   model [on|off]...
.model name sw  (vt=.. vh=.. ron=.. roff=..)
.model name csw (it=.. ih=.. ron=.. roff=..)
```

| Item | Behaviour |
| --- | --- |
| model name | required; a missing or undeclared model is an error (C: "Unable to find definition of model") |
| `on` / `off` | bare `IF_FLAG` setters in written order; the last wins (`SWparam`/`CSWparam`); `off` is the default |
| W controlling source | stored first as `control` (`ParameterKind::Instance`), resolved after elaboration like F/H (V, E or H branches; renamed hierarchically inside subcircuits) |
| `sw` / `csw` model tail flag | accepted ("just says that this is a switch") |
| defaults | `VT`/`VH`/`IT`/`IH` 0; `RON` gives 1 S; an omitted `ROFF` gives the circuit `gmin` (`.option gmin`, `SW_OFF_CONDUCTANCE`) |

Errors instead of C's silent behaviour: a number after the model (C ignores
it, `waslead`) is `Unsupported`; `level` on a switch model (C warns and
ignores it) is `Unsupported`; any other instance or model setter, `on=1`,
`RON`/`ROFF` of zero or without a finite conductance are positioned errors.
The writer emits `s1 n+ n- nc+ nc- model [flags]` and `w1 n+ n- vname model
[flags]` and refuses anything else (round trip and fixed point tested).

## State, phases and hysteresis

Each switch owns two state slots: the switch state with C's codes (0 really
off, 1 really on, 2 off inside the band, 3 on inside the band; 1 and 3 stamp
the on conductance) and the control value. Newton loads carry C's
`MODEINITF` phase (`spice_devices::IterationPhase`) and the previous load's
trial (C's `CKTstate0` survives between iterations); see the module docs of
`spice_devices::switch` for the full table. In short:

| Phase (C) | When | State |
| --- | --- | --- |
| `Junction`/`Fix` (`MODEINITJCT`/`MODEINITFIX`) | first `.op`/bias load; later loads until the iterate first converges | from the flag: `on` is really on above `VT+|VH|`, else on-in-band; `off` is really off below `VT-|VH|`, else off-in-band |
| `Predict` (`MODEINITTRAN`/`MODEINITPRED`) | first load of a timepoint or of a DC sweep point after the first | outside the band from the control; inside it the **accepted** state (`VH <= 0`: C's band mapping) |
| `Float` (`MODEINITFLOAT`) | every other load | outside the band from the control; inside it S keeps the **previous iterate**, W the **accepted** state; a change from the previous iterate marks the load nonconvergent (`CKTnoncon++`) |

`newton::solve_phased` never ends a solve on a load marked nonconvergent and
checks convergence with a `Float` reload, so a converged point is always one
whose switch states are stable. Gmin/source-stepping stages continue the
previous stage's trial in `Float`, as `cktop.c` does.

**Accepted state.** Only `Circuit::accept_point` commits the switch state:
transient trials, rejected steps and failed Newton iterations never change
what later trials read. Where C reads `CKTstate1` before any point was
accepted (a plain `.op`) its state vectors are zero, "really off"; the port
uses the same value. Consequences reproduced from C: inside a positive band a
W switch opens at an operating point whatever its `on` flag
(`switch_op`'s `w2`), and with `VH = 0` a control exactly on the threshold
maps the previous state (S: `MODEINITPRED` swaps really-on and really-off).

**DC sweeps** keep an accepted-state history from point to point
(`dctrcurv.c` rotates its state vectors), so hysteresis is visible across a
sweep. Each point after the first starts in `Predict`; its gmin/source
fallbacks restart in `Junction` against the same history.

**Transient**: the initial operating point's converged state (not a reload
from the flags) seeds the history; every timepoint starts in `Predict`.

## Timestep control

`Device::timestep_limit` is C's `DEVtrunc` for devices without charge
storage; the companion driver folds it into the truncation bound. For S
(`swtrunc.c`): while really off with the control rising below `VT + VH`, or
in any other state with the control falling above `VT - VH`, the next step is
limited to `(0.75 (ref - c) +/- 0.05) / (c - c_accepted) * dt`. C tests the
"really off" code only, so an off-in-band switch is limited like a closed one;
the port does the same. C's `cswtrunc.c` would apply the same rule with
`5e-5`, but `CSWload` never stores the control value (the store is commented
out, a `FIXME` in C), so W sets no limit in either engine.

## AC (deliberate divergence)

The small-signal conductance is that of the converged operating point's state
(codes 1 and 3 on), the same rule the DC load used. C's `ACan` instead reloads
with `MODEINITSMSIG`, which copies `CKTstate1` (still zero, "really off",
after a plain operating point) into `CKTstate0`, and `SWacLoad`/`CSWacLoad`
then treat every non-zero code (including off-in-band) as on. So C's AC sweep
sees a switch that is closed at the operating point as **open**. The opt-in
test `c_switches::ac_uses_the_operating_point_state_unlike_c` pins both values.
No AC golden is captured for switches.

## Verification

* C goldens (in `cargo xtask golden verify`): `switch_op` (`.op`: bands,
  flags, defaults, a W latch), `switch_dc` (`.dc` downwards through S/W
  bands with positive, negative and zero hysteresis), `switch_tran` (PULSE-
  and SIN-controlled S, charge sharing, a relaxation oscillator) and
  `switch_w_tran` (W sensing SIN and PULSE currents). The port reproduces C's
  accepted timepoints on these decks (identical point counts; worst error
  0.000 of the `compare::TRAN` bound).
* `spice-devices/tests/switches.rs`: elaboration errors, C defaults, trial
  isolation (dropped trials never change the accepted state), hysteresis and
  threshold crossings, flag/band decisions, `Float` nonconvergence, W branch
  sensing, `swtrunc.c` limits and the small-signal state.
* `spice-analysis/tests/switches.rs`: production `.op`/`.dc`/`.ac`/`.tran`,
  step control on a resistive ramp, the phased Newton contract and explicit
  diffsol/`uic` failures.
* Opt-in live C (`NGSPICE_BIN=... cargo test -p spice-analysis --test
  c_switches -- --ignored`): a resistive ramp (identical steps), negative
  hysteresis with a PULSE and an ON-flagged W, switches in a subcircuit, up
  and down DC sweeps, a chattering switch that fails with "timestep too small"
  at the same instant in both engines, and the AC divergence.

## Limits

* `.dc` sweep values are computed as `start + i step` while C accumulates
  `value += step`; for steps that are not binary-exact (for example `0.1`) the
  two differ by an ulp, which decides a control that lands exactly on a
  threshold differently. Binary-exact steps match C.
* C always tries a warm `MODEINITPRED` solve at a sweep point after the first
  and then `CKTop` (a direct `MODEINITJCT` attempt, then stepping). Without
  a warm-start limit (`itl2`, `trcvmaxiter=`) the port's per-point solve runs
  its direct attempt in `Predict` and then its own continuation strategies
  ([DC_CONTINUATION.md](DC_CONTINUATION.md)); the converged states agree on
  the verified decks.
* Not ported: switch noise (`swnoise.c`), pole-zero (`swpzload.c`),
  `@s1[i]`/`@s1[p]` queries (`swask.c`), C's `optran` fallback for operating
  points that do not converge, and the diffsol BDF backend.
