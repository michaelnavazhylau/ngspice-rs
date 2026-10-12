# `.sens`: DC and AC sensitivity analysis (#102)

Implemented for issue #102 (milestone M8) in `src/analysis/sens.rs` (driver)
and `src/devices/sensitivity.rs` (+ `sensitivity/*.rs`,
`nonlinear/sens.rs`; device descriptions). C references, read as behaviour
only: `sens_sens()` in `src/spicelib/analysis/cktsens.c`, the parameter
generator `sgen_init()`/`sgen_next()` in `cktsgen.c`, `senssetp.c`,
`dot_sens()` in `src/spicelib/parser/inp2dot.c`, and the devices' setters,
ask, setup, temperature and load routines.

```text
.sens v(n[,m])|i(vsrc) [filter ...] [dc | ac dec|oct|lin pts fstart fstop]
```

## Which C analysis

The reference build registers `.sens` as `SENSinfo`, whose job function is
`sens_sens()` (`analysis.c`). That is a **finite-difference** analysis: the
SPICE2 adjoint code (`*sload.c`, `*sset.c`, `*sacl.c`, `nisenre.c`) belongs to
the `WANT_SENSE2` `.sens2` analysis, which the reference build does not
compile. The port reproduces `sens_sens()`.

## Method (C's)

1. The DC operating point is solved as for `.op` (`CKTop`).
2. **DC**: `Y` is the operating-point Jacobian, the matrix C's last `CKTop`
   load leaves factored (the port reloads it with C's `DEVload` matrix,
   `TrialState::with_c_jacobian`, as `.tf` does) and `x` the operating
   point. **AC**: at every frequency C runs `CKTsetup`, `CKTtemp`, a load and
   `NIacIter` on the circuit as the previous frequency left it, so `Y = A +
   j omega E` and `x` are rebuilt from the devices' current (perturbed and
   restored) records.
3. For each parameter in `sgen` order: `DEVsetup` of the isolated instance,
   `DEVtemperature`, a load (`dY0`, `dI0`); the setter with `p + delta`
   (`delta = 1e-6 p`, or `1e-6` when `p = 0`: `Sens_Delta`, `Sens_Abs_Delta`,
   not configurable), `DEVtemperature`, a load (`dY1`, `dI1`); then the
   setter with `p` again and `DEVtemperature`. The column is
   `Y^-1 ((dI1 - dI0) - (dY1 - dY0) x)` at the output, divided by `delta`.
   DC loads are `DEVload` at the operating point, AC loads `DEVacLoad`.

So a sensitivity is a first-order forward difference (relative error about
`1e-6`), not an exact derivative.

## Parameters, names and order

* **Which**: every entry of the device's instance and model `IFparm` tables
  flagged `IF_SET|IF_ASK|IF_REAL` without `IF_VECTOR`, `IF_REDUNDANT` or
  `IF_NONSENSE` whose ask succeeds; a DC analysis also drops `IF_AC` and
  `IF_AC_ONLY` entries. The port lists them per device in C's table order
  (`SensitivityParameter`). Instances without a model card use C's default
  model of their type, whose parameters are listed too (`r1:rsh`, ...).
* **Order**: device types in `DEVices[]` order (B, Q, C, F, H, W, D, L, K, I,
  M, R, S, G, E, V), models in reverse order of creation, instances in reverse
  deck order; per instance the model parameters, then the instance
  parameters.
* **Names**: `<inst>:<kw>` for a model parameter, `<inst>` for the first
  `IF_PRINCIPAL` instance parameter, `<inst>_<kw>` otherwise; the rawfile
  wraps every name in `v(...)` with unit `voltage`, also for an `i(...)`
  output. Subcircuit instances keep C's flattened names (`r.x1.r2:rsh`).
* **Filters**: words between the output and `ac`/`dc` are `Sens_filter`
  patterns (`*`, `?`, `scan()` of `cktsens.c`). A filtered-out parameter is
  skipped entirely: neither perturbed nor restored.
* **Plot**: `Sensitivity Analysis` (`sens<n>` in a batch, run after `.noise`
  and before `.sp`). DC: one real point. AC: a complex plot with `frequency`.

AC frequencies follow `count_steps()`/`inc_freq()`, including two C defects:
`lin` *multiplies* by its step `(fstop - fstart)/pts` (`inc_freq` tests the
noise `LINEAR` constant, not `SENS_LINEAR`), and the `oct` count divides by
`M_LOG2E` instead of multiplying (`oct 1 1 16` gives 2 points).

## C's in-place perturbation, reproduced

C perturbs its live device structures, so the side effects of each step
persist to the next parameter, the next instance of the same model and the
next frequency. They shape C's numbers, and the port replays them: each
device is modelled as C's records of fields and *given* flags
(`SensitivityRecord`) with its setter, setup and temperature routines
(`DeviceSensitivity`), and the analysis keeps one record per model and per
instance for the whole run. Consequences visible in C's (and the port's)
output:

* Restoring a parameter marks it *given*. A resistor's `ac` then fixes
  `RESacConduct`, so at later frequencies its `resistance` no longer reaches
  the AC load (`r1` reads 0 after the first frequency); an instance `tc`
  overrides the model's `tc1` for every later parameter; `tce` switches the
  temperature factor to `1.01^(tce dT)`; `temp` disables `dtemp`.
* Non-idempotent setters: `VCCSparam`/`CCCSparam` multiply a gain by an `m`
  given *before* it, so after `m` was perturbed every gain restoration
  multiplies again (a never-given `m` reads 0 and zeroes the gain); a
  capacitor's `capacitance` asks the temperature-adjusted value times `m` and
  sets the nominal one.
* Parameters perturbed from an unset default: a resistor model `r` (0, a
  setter ignoring non-positive values), a V source `dc` without a DC value
  (the PULSE/PWL time-zero value is replaced by `1e-6`), a diode's `isr`
  (reads `1e-14`, turns the recombination current on and leaves it on) or
  `jtun`.
* Setup-only quantities do not follow a perturbation: a diode's series
  conductance `1/RS` (an `rs` sensitivity is exactly 0), its instance knee
  currents `IKF*AREA*M` (an unset `ikf`, perturbed, divides by a zero knee
  current: C reports NaN, and the port reports NaN in every unknown of the
  matrix block the NaN reaches), `NBV` defaulted to the `N` setup saw.
* RF port setters, switch thresholds and noise/SOA/geometry parameters are
  listed and perturbed; where C's load ignores them the column is exactly 0.

## Device coverage

| Device | DC | AC |
| --- | --- | --- |
| R (literal and model-backed) | yes | yes |
| C, L (literal and model-backed) | yes (all 0: their DC loads ignore every parameter) | yes |
| K | yes (nothing to perturb: `k` is `IF_AC`) | refused |
| V, I (not RF ports) | yes | yes |
| E, F, G, H | yes | yes |
| B (`asrc`) | yes | refused |
| S, W | yes | refused |
| D | yes | refused |
| Q (Gummel-Poon), M (MOS1, MOS3), XSPICE code models (POLY/TABLE), RF port V sources | refused (`NotYetPorted`) | refused |

The device hook is `Device::sensitivity(&ModelContext) ->
SpiceResult<Box<dyn DeviceSensitivity>>`; its default is an explicit
`NotYetPorted`, so a device whose C parameters are not reproduced is refused,
never left out of the list. The analysis stamps the stand-in devices a
description returns through `Circuit::load_device` (DC) and
`Circuit::assemble_small_signal_device` (AC); the diode stand-in is the
port's own `Diode` built from C's records.

## Divergences (explicit errors where C carries on)

* **AC with nonlinear devices, switches or K**: before every frequency C runs
  `CKTunsetup`/`CKTsetup` and a DC-mode load, which resets the device state,
  so diodes, transistors and switches are linearized at a reset state rather
  than at the operating point (a forward-biased diode is effectively absent),
  and a K coupling acts through the inductors' loads. The port refuses these
  decks.
* A BJT, MOS1, code model or RF port in the circuit (see above). C's BJT
  sensitivities are dominated by the `ibe`/`ibc` restore side effect (both
  left given at 0 disable the transistor for every later parameter).
* More than one `.sens` card in a deck (C shares the last card's filter and
  runs later cards on the perturbed circuit), and `.sp` together with
  `.sens` (C runs `.sp` on that circuit).
* An unknown output node or an `i(x)` whose device has no findable branch
  (C creates the node, or reads ground), a non-positive or nonfinite AC
  frequency (C would use the DC matrix at 0 Hz), trailing tokens after the
  sweep, an unknown stepping keyword, or a filter list that selects nothing
  (C writes no plot).
* C keeps going with a nonconverged operating point; the port propagates the
  failure.

## Verification

* `tests/sensitivity_analysis.rs`: closed-form divider derivatives
  (`dV/dR`, `m`, `scale`, `dV/dV1`), C's names and order, filters, current
  and differential outputs, an RC low-pass `dH/dC = -j w R H^2` over a decade
  sweep (with the sticky `ac` resistance), a diode's `is` sensitivity against
  a central difference of two `.op` solutions plus its zero `rs` and NaN
  `ikf`, batch order and every refusal.
* C goldens (`cargo xtask golden verify`): `sens_divider` (R/V/I/E/F/G/H, DC),
  `sens_ac` (R/C/L/V/I/E/F/G/H at 40 C, decade sweep), `sens_multi` (`.ac`,
  `.op` and a filtered `.sens` batch) under `compare::SENSITIVITY` (1e-8
  relative, 1e-12 absolute); `sens_hot` (60 C, geometry and TC resistors, a
  PULSE source, B source, switch, current output) and `sens_diode` (two diode
  models, tightened RELTOL) under `compare::SENSITIVITY_NONLINEAR` (1e-6
  relative, 1e-9 absolute; the floor covers forward differences of weak
  parameters that are a few ulps of the node voltage, see
  `xtask/src/compare.rs`).
* Opt-in live C (`NGSPICE_BIN=... cargo test --test c_sens_reference --
  --ignored`): DC linear and filtered decks, a hot behavioural/switch deck,
  diode decks with NaN, sidewall, breakdown and `.op` composition, and AC
  decks including `oct` and C's `lin` stepping, against `ngspice -b -r`:
  names **in order**, units, flags and values (NaN for NaN).
