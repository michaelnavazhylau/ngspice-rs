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
| Q (Gummel-Poon), M (MOS1, MOS2, MOS3, MOS6, MOS9), XSPICE code models (POLY/TABLE), RF port V sources | refused (`NotYetPorted`) | refused |
| J (JFET level 1, `jfet.c`) | yes | refused |
| J (JFET level 2, `jfet2.c`) | yes | refused |
| Z level 1 (MES, `mes.c`) | yes | refused |
| Z level 2 (MESA, `mesa.c`) | yes | refused |
| Z level 3 (MESA) | yes | refused |
| Z level 4 (MESA) | yes (including a shifted instance temperature) | refused |
| Z level 5 (HFET1, `hfet1/hfet.c`) | yes | refused |
| Z level 6 (HFET2, `hfet2/hfet2.c`) | yes | refused |

The device hook is `Device::sensitivity(&ModelContext) ->
SpiceResult<Box<dyn DeviceSensitivity>>`; its default is an explicit
`NotYetPorted`, so a device whose C parameters are not reproduced is refused,
never left out of the list. The analysis stamps the stand-in devices a
description returns through `Circuit::load_device` (DC) and
`Circuit::assemble_small_signal_device` (AC); the diode stand-in is the
port's own `Diode` built from C's records.

### The `j` and `z` families (#164)

**Ported: JFET levels 1 and 2, the MESFET (`z` level 1), MESA levels 2-4 and
HFET1 (`z` level 5).** Each family replays C's own sequence, the way the diode
does, and each has a committed C golden with every enumerated parameter:

| Family | `sgen` parameters | Fixture |
| --- | --- | --- |
| `jfet.c` | 20 model + 4 instance | `sens_jfet` (116 vectors, two models, both polarities) |
| `jfet2.c` | 36 model + 4 instance | `sens_jfet2` (148 vectors) |
| `mes.c` | 10 model + 1 instance in DC (12 model in AC: `cgs`/`cgd`) | `sens_mesfet` (90 vectors) |
| `mesa.c` level 2 | 61 model + 8 instance | `sens_mesa` (69 per device, two models, one at `dtemp=10`) |
| `mesa.c` level 3 | 61 model + 8 instance | `sens_mesa3` (69 per device, two models, one at `dtemp=10`) |
| `mesa.c` level 4 | 61 model + 8 instance | `sens_mesa4`, and `sens_mesa4_ts` at `ts=40` |
| `hfet1/hfet.c` | 63 model + 7 instance | `sens_hfet1` (218 vectors, n- and p-channel) |
| `hfet2/hfet2.c` | 36 model + 6 instance (`m` is not askable) | `sens_hfet2` (194 vectors, a shared n-channel model and a p-channel at `temp=50`) |

The parameter lists are C's, not the card's, and three filters decide them:
`set_param()`'s `(IF_SET|IF_ASK|IF_REAL)` test (so the `vto`/`pb`/`tphib`
`IOPR` aliases, ask-only `gd`/`gs`/`depl_cap`/`vcrit` and integer `nlev` are
skipped), `IF_AC` (`cgs`/`cgd`/`cds`, and the `icvds`/`icvgs` instance
temperatures, appear only in an AC run), and - easy to miss - `sens_getp()`
must succeed, so a settable parameter the device cannot **ask** is skipped
too: `mes.c`'s `kf`/`af` are `IOP` but `mesmask.c` has no case for them, and
`MES_M` is `IOPUR`, so a MES `.sens` has no `z1_m` column at all.

C quirks the replay reproduces (each measured against the reference binary):

- **`jfettemp.c`/`jfet2temp.c` branch on *given* flags**: `xti`, `vtotc` and
  `betatce` select their formula by the flag, not by the value, so perturbing
  `xti` on a card that never set it switches `IS(T)` from
  `is exp(ratio1 eg/vtn)` to the `pow(ratio1+1, xti)` form. The record keeps
  C's setup default (`xti = 3`) with `given = false`, and the port's model
  keeps the same three fields as `Option`s.
- **`jfet2`'s `hfgam` defaults to `lfgam`** (`jfet2parm.h`), re-derived by
  every `DEVsetup`, so perturbing `lfgam` also moves `hfgam` unless the card
  gave it. In the record that *given* flag is `hfgam != lfgam`, which is
  behaviourally exact.
- **`mestemp.c` has no given-flag branches** and MES has no instance
  temperature, so a MES record needs no temperature replay at all.

Two limits are recorded rather than reproduced:

- **A JFET with `M != 1`.** C's `.sens` disagrees with its own finite
  difference for the *instance* parameters: measured on
  `j1 2 3 0 jm m=3`, C reports `j1_m = -3.000000e-1` while perturbing `m` by
  1 ppm in two `.op` runs gives `-1.000000e-1` (the `m` column is exactly
  `M` times its own derivative), and `j1_area = -2.0000e5` against a finite
  difference of `-3.0e-1` (the extra term is `-(M-1) IR`). At `M = 1` every
  column agrees exactly, so the fixtures keep `M = 1`. The port computes
  each parameter from an independently rebuilt load, which is the finite
  difference C's own solver agrees with.
- **Cancellation-dominated columns.** A parameter whose sensitivity is a
  near-cancellation (JFET2's `delta`, `mxi`, `xi`, `z`, `vbi` at exotic
  values) amplifies last-digit differences of the derived quantities by the
  cancellation ratio, so those columns agree only to a few digits. The
  fixture keeps JFET2's feedback family at its defaults; the columns are
  still enumerated and perturbed.

**MESA levels 2, 3 and 4 are ported** (`sens_mesa`, `sens_mesa3`,
`sens_mesa4`, `sens_mesa4_ts`; 61 model and 8 instance parameters per device,
69 columns each), as are HFET1 and HFET2 - see
[the next section](#mesa-levels-2-3-and-4-hfet1hfet2).

Two earlier blockers are now reproduced or routed around, and the notes that
recorded them are superseded:

- **`MESAphib`/`MESAphib1`'s asymmetry** (`mesamparam.c` stores `value *
  CHARGE`, `mesamask.c` returns the stored joules) is replayed exactly: the
  fixture sets `phib=0.5/CHARGE` so the restored barrier stays physical, and
  the port's `set` applies the same factor.
- **Operating-point branches**: a `.sens` deck needs both a stiff supply (to
  land on C's operating point) and a responsive node (to have a signal), and
  the internal `#gate` node provides the second without the first's ambiguity.
  `sens_mesa`'s two devices sit behind stiff drain supplies with the gate
  driven through 1 k; `sens_mesa3`/`sens_mesa4` sense the drain current
  instead.

### MESA levels 2, 3 and 4, HFET1/HFET2

**MESA levels 2, 3 and 4 are ported and verified**: `sens_mesa` (level 2,
`sens_mesa` senses the `#gate` node, so it uses
`compare::SENSITIVITY_CANCELLED`), `sens_mesa3` (level 3, one device at the
circuit temperature and one at `dtemp=10`), `sens_mesa4` (level 4) and
`sens_mesa4_ts` (level 4 at `ts=40`, the combination that was refused).

The earlier level-3/level-4 refusals were wrong about *why*, and the corrected
diagnosis is worth recording because the first reading was plausible and false:

- **Level 3's `du`/`th` rows were never the problem.** On the degenerate deck
  they were measured on they sat 16%/24% from C, but that deck sensed a ~1 uV
  gate node; on a saturated bias the two columns agree to 1e-11 and match both
  implementations' own finite differences. What *did* separate the port from C
  on a shifted instance temperature was `del`/`xchi` - and level 2, shipped as
  verified, shows the same two rows the same way (8.4%/22% at `dtemp=10` with
  `rg=1k`), snapping to *bit-identical* agreement when the same deck is run
  with `rg=1.01k`. That is the cancellation class this document already
  records for `sens_mesa`, not a derivation gap: the operating point agrees to
  1e-12 and only a row that is a difference of much larger terms moves.
- **Level 4's `rtc1`/`rtc2` rows were a real defect, in the device and not in
  `.sens`.** At `ts=40` they came out at 0.808x C's (-4.459e-3/-5.796e-2
  against -5.519e-3/-7.174e-2), and a finite difference of two `.op` runs - no
  `.sens` machinery involved - reproduced the same 0.807 ratio, so the port's
  *model* was wrong. `mesaload.c`'s `mesa2`/`mesa3` read `RS`, the
  temperature-scaled `here->MESAtRsi` that `mesatemp.c` computes from
  `rsi`/`rtc1`/`rtc2`, while the port read the raw `model.rsi` in `f`/`d`,
  `ddevgte`, `p` and `delisatmgchim` (`mesa3`'s `h` is the one place C itself
  uses the raw field). Nothing showed up at `tnom` or with `rtc1=0`, which is
  why every earlier level-3/4 fixture passed. With the scaled value in place
  every column of `sens_mesa4_ts` agrees, and
  `tests/mesa.rs::the_channels_use_the_temperature_scaled_intrinsic_resistances`
  pins the level-2/3/4 drain currents against the reference binary.

Getting there needed things that are easy to get wrong, and all were measured
against the reference binary:

- **`mesamparam.c` stores five parameters in kelvin**: `tlambda`, `teta0`,
  `teta1`, `tmu` and `tf` are `value + CONSTCtoK` in C's model, while
  `mesatemp.c` then uses them as if the deck value were the coefficient. C's
  `.sens` therefore perturbs `teta1 = 0` to 273.15, not to 1e-6: the `teta1`
  column is the finite difference between 273.15 K and 0, which is exactly what
  a card `teta1=1e-6` reproduces (`i(vids)` changes by -4.02e-11, i.e.
  -4.024e-5 per unit). Restoring the *asked* value re-adds the offset, so after
  the `teta1` row the model holds 273.15 K and after the `tmu` row 573.3 K -
  which is the only reason C's later `xtm0` column is non-zero at all
  (`pow(ts/tmu, xtm0)` is exactly 1 while `tmu` is its 300.15 K default).
  The port's `set` reproduces the offset; earlier notes calling these rows
  "C artefacts" were wrong, and so was the claim that C's `.sens` disagrees
  with its own finite difference - it does not, once the card carries the same
  value.
- **`mesaask.c` answers `td`/`ts` in Celsius while `mesaparam.c` stores them in
  kelvin**, the same asymmetry as `hfetask.c`'s `temp`: `MESAts = value +
  CONSTCtoK` on every setter call, `MESAts - CONSTCtoK` on every ask. The
  perturbation therefore steps the Celsius value and re-adds 273.15. The step
  size does not cancel out of C's answer exactly - `.sens` divides a fixed-bias
  linearisation by `delta`, so a nonlinear row keeps an O(delta) residue - and
  modelling the kelvin field directly left `td`/`ts` 2.2e-3/6.4e-6 off in
  `sens_mesa3`'s deck until `MesaSensitivity::ask`/`set` mirrored C.
- **The sensed quantity decides whether a row is comparable at all.** `sens_mesa`
  senses a ~1e-6 V gate node, where a level-4 row is a cancellation-limited
  difference of last digits (that, not a derivation gap, is what made `epsi`
  read 0.98% and `gamma` 4.0% off); `sens_mesa3`/`sens_mesa4` sense `i(vids)`,
  which is O(mA), and every column agrees to 1e-6. Near cut-off the model's
  exponential tails additionally make `.sens`'s fixed-bias linearisation
  differ from a re-solved operating point by orders of magnitude, so a `.sens`
  fixture must also sit well inside the saturated region.

**HFET1 is ported and verified** (`sens_hfet1`, 218 columns: `hfet.c`'s 63
`IOP` model setters plus 7 instance ones per device, one n-channel and one
p-channel). Two of its C quirks are reproduced rather than smoothed over:

- **`hfettemp.c` multiplies the stored threshold by the model type on every
  call** (`HFETAthreshold *= HFETAtype`). A p-channel card therefore alternates
  its `vt0` - and everything derived from it - each time the temperature
  routine re-runs, which the `.sens` walk does three times per parameter. The
  port keeps the running product in the record (seeded from `threshold / pol`
  so the first temperature call lands on C's value) and `Model::threshold` now
  holds the product instead of re-applying the type inside `derived`.
- **`hfetask.c` answers the instance temperature in Celsius while
  `hfetparam.c` stores it in kelvin.** The `temp` column is a relative step on
  the Celsius value followed by a `+273.15` store, so modelling only the kelvin
  field would perturb across 273 K and be millions of times too large.

**HFET2 (level 6) is ported and verified** (`sens_hfet2`): `hfet2.c`'s 36
`IF_REAL` model setters and six instance ones, with a shared n-channel model
carrying an instance at `dtemp=10` and a p-channel at `temp=50` whose drain
current returns through the sensed supply. 194 columns, all inside 1e-6
(`compare::SENSITIVITY_NONLINEAR`).

Four C behaviours the replay reproduces, each measured against the reference
binary:

- **`d2`/`eta2` are values, but their *given* flags are a topology switch.**
  `hfet2temp.c` fills `N02 = EPSI*ETA2*vt/2/CHARGE/D2` under
  `if(model->HFET2eta2Given)` alone, `hfet2load.c` reads `nsc` under
  `eta2Given && d2Given`, and `HFET2mParam` marks a parameter given on every
  setter call - nothing clears the flag again. Perturbing `d2` therefore
  *enables* the `nsc` branch in the perturbed load, so that row is a difference
  of two model topologies rather than a derivative whenever the card gave only
  one of the pair. Measured on `vds=2`, `vgs=0.2`, `.sens i(vids)`:

  | card | `v(z1:d2)` | `v(z1:eta2)` |
  | --- | --- | --- |
  | `eta2=2 d2=0.3u` (both) | -2.4138e2 | -7.2527e-5 |
  | `eta2=2` | -8.3209e8 | -7.7632e-5 |
  | `d2=0.3u` | 0 | -9.8502e1 |
  | neither | 0 | -8.3209e1 |

  The zeros are exact rather than small: with `N02 = 0` the perturbed channel
  dies at `hfeta2`'s `1e-38` cutoff and the row is the operating point's
  residual. With both given the row *is* the honest derivative - `.sens`
  -2.4137675792e2 against a forward difference of two `.op` runs at `h=1e-6` of
  -2.4137675795e2 (1.1e-10), unchanged over a 10x `rg` change - so the replay
  takes `eta2_given`/`d2_given` from the *records* (where
  `SensitivityRecord::set` leaves them set) instead of from the original card,
  and `tests/hfet2.rs` pins all four cards. The earlier "`d2` is 1.9e-3 from
  C" measurement came from a replay that did not survive review; with the
  records carrying the flags exactly as the walk leaves them, the row matches
  C to 1e-8 (the fixture's `v(z1:d2)` is -241.586046156546).
- **`hfet2ask.c` answers `temp` in Celsius while `hfet2param.c` stores
  kelvin**, the same asymmetry as HFET1's pair: `ask` subtracts 273.15 and
  `set` adds it, so a `temp=50` card is stepped by 50e-6 K rather than 323.15.
- **`m` passes the table filter but not the ask.** `hfet2ask.c` has no
  `HFET2_M` case (`hfetask.c` does have `HFETA_M`), so C prints
  `GET ERROR: HFET2:<model>:<inst> -> param m (11)` and reports no multiplier
  column; the instance list is six entries, and `tests/hfet2.rs` asserts
  `v(z1_m)` is absent while the 36 model setters and the other five instance
  ones are present.
- **`HFET2_VT1` is computed and read nowhere** - `hfeta2` uses `VT2`, the raw
  threshold, not `VT1` - so its column is exactly zero. Both are model fields
  that follow the raw `VTO` (not the type-scaled `TVTO`) whenever they are not
  given, which keeps a p-channel card's thresholds negative while its `TVTO` is
  positive.

`RSI`/`RDI` are the raw model fields - HFET2 has no `rtc1`/`rtc2` - so the
`tRsi` defect recorded above has no analogue here.

### The `j` and `z` families: the original gap note (#164)

*Historical: this is the note that opened the work. Every family below now has
a `DeviceSensitivity` implementation; the coverage table above is the
authoritative status.*

C's modern `.sens` does **not** use the SPICE2 adjoint `*sload.c` routines, so a
device whose entry point sets `DEVsenSetup = NULL` is still enumerated:
`cktsens.c` walks every real parameter of every instance and model
(`cktsgen.c`'s `sgen_init`/`sgen_next`), perturbs it through `DEVparam`/
`DEVmodParam`, calls `DEVtemperature`, reloads the device and solves. Measured
with the reference binary on `z1 2 3 0 zm l=1u w=20u`, `.model zm nmf(level=2)`,
`vgs=0`, `vds=0.5` and `.sens v(2)`, it exits 0 and its `sens1` plot carries 61
`z1:*` vectors (`z1:ag`, `z1:alpha`, `z1:astar`, `z1:beta`, `z1:cas`, ...).
`j1` is refused by the port the same way for the same reason.

What each family needed, in this order, is exactly what
[`nonlinear/sens.rs`](../src/devices/nonlinear/sens.rs) does for the diode:

1. `mesamparam.c`/`mesaparam.c` (and `jfet.c`/`jfet2.c`, `mes/`, `hfet.c`)
   `IFparm` tables in C's order, filtered to the entries `set_param()`
   accepts;
2. the `*setup.c` default and *given*-flag side effects, which persist from one
   perturbed parameter to the next (`mesasetup.c` alone fills ~30 fields);
3. `*temp.c`'s derived values;
4. a load that builds the port's own device from the records, so the stamp is
   the perturbed one.

The families share the card grammar but not the channel or the tables, so this
was one slice per family rather than one generic helper. Every `z` level now
has one; a device type without one (Q, the MOS levels, the code models) still
refuses through `Device::sensitivity`'s `NotYetPorted` default, naming its own
`DEVparam`/`DEVtemperature`/`DEVload` reference, so a deck that asks fails
loudly.

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
  `xtask/src/compare.rs`). The `j`/`z` family fixtures are there too:
  `sens_jfet`, `sens_jfet2`, `sens_mesfet`, `sens_hfet1`, `sens_hfet2`,
  `sens_mesa`, `sens_mesa3`, `sens_mesa4` and `sens_mesa4_ts`.
* Opt-in live C (`NGSPICE_BIN=... cargo test --test c_sens_reference --
  --ignored`): DC linear and filtered decks, a hot behavioural/switch deck,
  diode decks with NaN, sidewall, breakdown and `.op` composition, and AC
  decks including `oct` and C's `lin` stepping, against `ngspice -b -r`:
  names **in order**, units, flags and values (NaN for NaN).
