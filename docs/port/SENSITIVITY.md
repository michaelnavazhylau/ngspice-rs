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
   point. **AC**: at every frequency C runs `CKTunsetup`/`CKTsetup`,
   `CKTtemp`, a DC load and `NIacIter` on the circuit as the previous
   frequency left it, so `Y = A + j omega E` and `x` are rebuilt from the
   devices' current (perturbed and restored) records, and every device is
   linearized at a **reset state**, not at the operating point (see below).
3. For each parameter in `sgen` order: `DEVsetup` of the isolated instance,
   `DEVtemperature`, a load (`dY0`, `dI0`); the setter with `p + delta`
   (`delta = 1e-6 p`, or `1e-6` when `p = 0`: `Sens_Delta`, `Sens_Abs_Delta`,
   not configurable), `DEVtemperature`, a load (`dY1`, `dI1`); then the
   setter with `p` again and `DEVtemperature`. The column is
   `Y^-1 ((dI1 - dI0) - (dY1 - dY0) x)` at the output, divided by `delta`.
   DC loads are `DEVload` at the operating point, AC loads `DEVacLoad`.

So a sensitivity is a first-order forward difference (relative error about
`1e-6`), not an exact derivative.

## The AC reset state (#102)

`CKTunsetup` frees the state vectors and solution, `CKTsetup` allocates them
zeroed (`NIreinit`), and the DC load that follows (`CKTmode` is still
`MODEDCOP|MODEINITFLOAT` from `CKTop`) sees all-zero node voltages. So each
`DEVacLoad`, at the frequency's solve and in every parameter's base and
perturbed load, reads:

* from `CKTstate0`, what that load stored at zero junction voltages:
  diode `conduct`, BJT `gpi`/`gmu`/`gm`/`go`/`gx`/`gdsub`, JFET/MESFET/HFET2
  `gm`/`gds`/`ggs`/`ggd`, a switch's state; MOS `gm`/`gds`/`gmbs`/`gbd`/`gbs`
  and the mode are instance fields the same load writes. These come from the
  records as they were at the frequency's `CKTtemp` and do **not** follow a
  parameter's perturbation, so channel and junction parameters (`is`, `bf`,
  `vto`, `kp`, `beta`, ...) have exactly zero columns, and a forward-biased
  diode is its zero-bias conductance `Is/Vt + gmin`;
* zero for the state slots only a small-signal or transient load writes: every
  charge capacitance (`DIOcapCurrent`, BJT `cqbe`..., MOS Meyer halves, JFET
  `qgs`/`qgd`);
* the temperature-derived and setup fields from the current records: series
  conductances, MOS overlap capacitances (whose `cgso`/`cgdo`/`cgbo` and `w`/`m`
  columns are `j omega` times geometry), the BJT excess-phase factor
  `PTF/(180/pi) TF` (with TF given, PTF's perturbation rotates `gm`), `m`;
* B sources: `ASRCsetup` reallocates `ASRCacValues` (zeroed) every time and
  only a small-signal load fills it, so a B source keeps only a voltage
  source's branch incidences;
* K: `MUTask` answers `MUTfactor` (`M = k sqrt(|L1 L2|)`) for `k`, so the walk
  sets the coupling to `M + delta` and restores it to `M`. The `k` column is
  that difference quotient (about `-3e10` on `sens_ac_mutual`), and from the
  next frequency on the coupling has collapsed to `M`.

`DeviceSensitivity::reset_ac_load` builds a stand-in (`ResetStamps` or a
family's own) that stamps exactly the C load's elements in C's order from the
reset records and the current ones. Two fields escape the reset: MOS
`capbd`/`capbs` are instance fields written only by `MODEINITSMSIG` or
transient loads, so they hold whatever an earlier `.op`, `.ac`, `.tran`,
`.pz`, `.disto` or `.noise` of the deck left (measured: a `.op` before the
`.sens` changes C's MOS columns; `.dc` and `.tf` do not). The last of those
to run decides (`Circuit::completed_analyses`, in C's `analInfo` order): none
leaves C's zero initialisation; `.op`, `.ac`, `.pz`, `.disto` or `.noise`
leave the small-signal load's values at the deck's operating point, which
`.sens` solves again identically, so the port evaluates them there with the
circuit's own device (`AcReset::small_signal`, verified by `sens_ac_mos_op`);
after a `.tran` they hold the final accepted load's values. The companion
driver records that load's state (including the MOS instance quantities) only
after the complete transient succeeds, for both stored and streamed output.
`Circuit::inherit_from` carries the snapshot to the next analysis, separately
from the DC/small-signal state; `.sens` takes each MOS's `capbd`/`capbs`
directly from its slots (`AcReset::transient`). It does not recompute them at
the final solution, which can be one Newton iterate beyond the final load.
The snapshot is valid only for the same device/state layout. Library callers
must inherit the actual circuit, not just copy its completed-analysis markers;
a missing snapshot still returns `NotYetPorted` instead of substituting the
operating point. `sens_ac_mos_tran` verifies `.op` → `.tran` → `.sens`.

Other families read only reset quantities. JFET2's `PSacload` runs at the
reset's zero `vgs`/`vgd`/`cd` with the current model coefficients, so at
`vds = 0` its `TAUD`/`TAUG` terms vanish. MESA's `mesaacl.c` reads the
`gm0`..`gds0`/`delidgch0`.. fields the reset load wrote, and its `LAMBDA`
blend multiplies the reset's zero `vds`. MESA level 4's `mesa3` never writes
those fields, so its AC load has no `gm`/`gds` in any analysis; under `.sens`
that is reproducible, unlike `.ac` at an operating point (refused as H2).
HFET1 stamps `CDS * CKTomega` and its `KAPPA`/`FGDS`/`DELF` output
conductance from the current records.

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
| K (two inductors) | yes (nothing to perturb: `k` is `IF_AC`) | yes (`k := M` quirk); a card over more than two inductors is refused |
| V, I (not RF ports) | yes | yes |
| E, F, G, H | yes | yes |
| B (`asrc`) | yes | yes (zeroed derivatives) |
| S, W | yes | yes (zero-control state) |
| D | yes | yes (zero-bias conductance, no capacitance) |
| Q (Gummel-Poon) | yes (`RC == 0`; with `RC != 0` C's Kull quasi-saturation changes the `gamma` row) | yes (`RC == 0`), with the `IF_AC` capacitance family and excess phase |
| M (MOS1, MOS2, MOS3, MOS6, MOS9) | yes, bounded by valid perturbations (see below) | MOS1/2/3/9 yes, including after `.tran` (final-load `capbd`/`capbs` carried between analyses); MOS6 refused (no C AC load) |
| XSPICE code models (POLY/TABLE), RF port V sources | refused (`NotYetPorted`) | refused |
| J (JFET level 1, `jfet.c`) | yes | yes |
| J (JFET level 2, `jfet2.c`) | yes | yes (`PSacload` at the reset state) |
| Z level 1 (MES, `mes.c`) | yes | yes |
| Z level 2 (MESA, `mesa.c`) | yes | yes |
| Z level 3 (MESA) | yes | yes |
| Z level 4 (MESA) | yes (including a shifted instance temperature) | yes: no `gm`/`gds` (`mesa3` never writes the fields `mesaacl.c` reads) |
| Z level 5 (HFET1, `hfet1/hfet.c`) | yes | yes |
| Z level 6 (HFET2, `hfet2/hfet2.c`) | yes | yes |

The device hook is `Device::sensitivity(&ModelContext) ->
SpiceResult<Box<dyn DeviceSensitivity>>`; its default is an explicit
`NotYetPorted`, so a device whose C parameters are not reproduced is refused,
never left out of the list. The analysis stamps the stand-in devices a
description returns through `Circuit::load_device` (DC) and
`Circuit::assemble_small_signal_device` (AC); the diode stand-in is the
port's own `Diode` built from C's records.

### The `j` and `z` families (#164)

**Ported: JFET levels 1 and 2, the MESFET (`z` level 1), MESA levels 2-4,
HFET1 (`z` level 5), HFET2 (`z` level 6), the Gummel-Poon BJT and the classic
MOS levels 1/2/3/6/9.** Each family replays C's own sequence, the way the diode
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
| `bjt/bjt.c` | 103 model + 6 instance (the `IOPA` family is AC-only) | `sens_bjt` (291 vectors, an NPN at `dtemp=-10` and a PNP at `dtemp=20`, emitter-sensed) |
| `mos1/mos1.c` | 23 model + 14 instance (IC voltages are DC here; levels 2/3/9 make them AC-only) | `sens_mos1` (146 vectors) |
| `mos2/mos2.c` | 32 model + 11 instance | `sens_mos2` (164 vectors) |
| `mos3/mos3.c` | 36 model + 11 instance (three have no setter) | `sens_mos3` (176 vectors) |
| `mos6/mos6.c` | 29 model + 11 instance (`TNOM` asked in kelvin, geometry asks scaled by `m`) | `sens_mos6` (155 vectors) |
| `mos9/mos9.c` | 35 model + 10 instance (no `DTEMP` ask) | `sens_mos9` (170 vectors) |

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

### BJT (`Q`, Gummel-Poon)

**The Gummel-Poon BJT is ported and verified** (`sens_bjt`): `bjt.c`'s 103
model `IF_REAL` setters and six instance ones, over an NPN at `dtemp=-10` with
`ISE` above 1e-4, `rb`/`re`/`irb`/`rbm`, area factors and a `BF` polynomial,
and a PNP at `dtemp=20` with `tnom=50`. 291 columns, all inside 1e-6
(`compare::SENSITIVITY_NONLINEAR`). The fixture senses the **emitter** current:
a collector-side ammeter hides `BF`/`ISE`/`NE`, which move the base current,
because `Ie = Ic + Ib`.

The DC list is C's tables minus the usual filters: the `IOPA` capacitance
family (`cje`, `vje`, `mje`, `tf`, `xtf`, `vtf`, `itf`, `ptf`, `cjc`, `vjc`,
`mjc`, `xcjc`, `tr`, `cjs`, `vjs`, `mjs`) is `IF_AC` and appears only in an AC
walk, `icvbe`/`icvce` are `IOPAU` for the same reason, `ic` is `IF_REALVEC`,
`off`/`sens_area` are flags, the probe entries are ask-only, the `IOPR` aliases
(`tref`, `va`, `ik`, `c2`, `vb`, `c4`, `pe`, `me`, `pc`, `mc`, `csub`, `ccs`,
`ps`, `ms`) are redundant, and `subs`/`tlev`/`tlevc`/`quasimod` are integers.
The list keeps parameters the port refuses when a card gives them (`rco`, `vo`,
`gamma`, `qco`, `vg`, `cn`, `d`, `rth0`, the SOA limits): C enumerates them and
their columns are zero on the fixture's card, so the port reports zero records
rather than dropping columns.

Four C behaviours the replay reproduces, each measured:

- **`DEVsetup` runs inside the per-parameter loop** (`cktsens.c:495`), so
  `bjtsetup.c`'s rewrites re-run for every one of the 108 parameters. The one
  that shows is the leakage scaling
  (`if (BJTleakBEcurrentGiven && BJTleakBEcurrent > 1e-4) BJTleakBEcurrent =
  BJTsatCur * BJTleakBEcurrent`). It is not idempotent, but it fires at most
  once per card because the product drops below the threshold. Measured on a
  `vbe = 0.7 V` probe with `IS = 1e-16`:

  | card | `v(q1:ise)` | `v(q1:is)` |
  | --- | --- | --- |
  | no leakage | -5.37506278e8 | 1.04351068e12 |
  | `ise=1e-3` | -5.37505669e8 | 1.04351022e12 |
  | `ise=1e-9` | -11.1332 | 8.5957e7 |

  These are not cancellation: at `IS = 1e-16` the scale of `dI/dIS` really is
  `I/IS`, and perturbing a zero leakage by C's absolute step switches the
  B-E leakage exponential on. A card above 1e-4 is a *different* device after
  the first setup, which is why its columns differ from the `ise=1e-9` row.
- **`BJTemp` also writes model fields**, and runs before every base and
  perturbed load: an ungiven `tnom` follows `CKTnomTemp`, an ungiven `rbm`
  follows `rb` (so the `rb` column moves `rbm` too - the port's `Model` now
  keeps `rbm_given` for this), `fc` is clamped to 0.9999 when given and 0.5
  otherwise, and the `NKF`/`XCJC` clamps live in setup.
- **`sens_setp()` hard-codes one parameter out**: `!(sg->dev == 2 && pid ==
  147)` skips `RCO` because setting it re-allocates nodes mid-analysis, so both
  `rco` columns are exactly zero, even on a card that gives `rco`.
- **`tnom` and `temp` are the Celsius/kelvin pair** (`bjtmask.c`/`bjtask.c`
  answer `- CONSTCtoK`, `bjtmpar.c`/`bjtparam.c` store `+ CONSTCtoK`), the same
  asymmetry HFET1's `temp` has.

**Refused: `RC != 0`.** `bjtsetup.c` then creates the internal `collCX` node,
and C's walk marks `VO`/`GAMMA`/`QCO` given as it passes them, which switches
the Kull quasi-saturation terms of `bjtload.c` on in the *perturbed* loads. The
port does not model that physics (the parameters are `UNPORTED_MODEL`), and the
effect is not a rounding difference: on a `rc=40` card C answers
`v(q1:gamma) = 6.52e-1` where the port can only answer 0. Every other column of
that card agrees, and a card without `RC` has no `collCX` node for the terms to
use - which is why the rest of the table verifies.

**Correction to the original single-sided IBE/IBC diagnosis:** the large
columns were not residual noise. The port had kept only the pair when both
setters were given, discarding a lone setter's value and flag. Both C and the
port ignore a lone setter in the initial physical model, but `.sens` can mark
the missing partner given later. The model now retains each setter separately
and the replay reconstructs the pair from the current flags. On the default-
tolerance `ibc=2e-14` probe, `q1:ibe` is 5.433385e11, `q1:ibc` is 4.100676,
`q1:vaf` is 0.0860111 and `q1:var` is -0.0139889; those formerly divergent
columns now match C. Two regression tests pin the independent records and the
large columns for both single-sided cards.

Tiny *given* values can still produce genuinely cancellation-limited rows:
`value*1e-6` may be below the resolution of the load subtraction, so near-zero
columns need the documented absolute floor. The single-IBE probe's own IBE
column, for example, is about -2e-9 in C versus zero in the port, even while
its 2.3868e11 VAF column agrees. This remaining numeric limit is distinct from
the fixed record-loss defect.

### Classic MOS (`M`, levels 1/2/3/6/9)

DC record replay is implemented for all five classic levels, with one shared
lifecycle in `devices::mos::sens` and level-owned tables, field mappings and
process updates. The new `sens_mos1`/`sens_mos2`/`sens_mos3`/`sens_mos6`/
`sens_mos9` goldens contain **146/164/176/155/170** vectors respectively;
every column verifies under the existing `SENSITIVITY_NONLINEAR` bound
(1e-6 relative plus 1e-9 absolute). Each card has two NMOS instances sharing a
process-derived model, independent bulk bias, sheet resistance, a multiplier
and shifted instance temperature, plus a PMOS with explicit channel parameters,
series resistances and a distinct nominal temperature. All source currents
return through the sensed branch. `tests/sensitivity_mos.rs` checks the tables,
values, square-law derivatives and explicit invalid-perturbation failures;
`c_sens_reference` also checks batch-mode vector order against live C.

The replay preserves several non-derivative behaviours:

* **Restoration changes resistance precedence.** Setting and restoring an
  absent RD/RS marks it given at zero, so later `mosXtemp.c` calls suppress
  RSH rather than using the original card's sheet resistance. The setup-time
  internal nodes remain; a perturbed load must not run the card validator or
  allocate new nodes.
* **Process fields persist in C's model record.** VTO/KP (KC for level 6),
  PHI/GAMMA and mobility are updated by temperature and can become fixed when
  restored. Setup resets only fields still ungiven. Instance TEMP and model
  TNOM normally ask Celsius and set kelvin.
* **MOS6 TNOM is the exception:** `mos6mask.c` returns kelvin while
  `mos6mpar.c` still adds 273.15. The fixture's `m1:tnom` column is about
  1.2889, not the physical temperature derivative. `mos6ask.c` also multiplies
  W/AD/AS/PD/PS/NRD/NRS by M, but their setters do not undo that scaling;
  restoration therefore changes later columns. LAMBDA feeds LAMBDA0 only in
  setup, so its own column is zero. NVTH and PS are unused zero columns.
* **MOS3/9 DELTA asks a scaled narrow factor but sets raw DELTA.** The fixture
  deliberately uses DELTA=10 and ETA=0.1 so the very small perturbation is
  resolvable without relaxing tolerances. XD/ALPHA/INPUT_DELTA have asks but
  no model setter cases; C prints SET ERROR and continues with unchanged
  fields, producing zero columns. Those entries remain in the table.
* **MOS9 has no DTEMP ask case.** C drops that GET ERROR column, even though
  the setter exists, so it is absent from the port's table too.
* **A `.model` card's instance parameters become defaults for every instance,
  but `m` is discarded** (`inpgmod.c`: "no instance parameter default for
  level and multiplier"). The `sens_mos1` fixture carries `m=3` on its
  n-channel `.model` next to an instance-level `m=2`, so the golden covers the
  rule: applying the model-level value would scale every current column by
  three. `tests/sensitivity_mos.rs` and `tests/m10_mos2.rs` pin it, and a
  *filtered* walk that never selects the unset `NSUB` matches C as well
  (`m1:kp`/`m1:vto`/`m1_l` on level 2, `m1:theta`/`m1_w` on level 3) -- the
  documented workaround for the bound below.

**Still bounded, not full MOS parity:** a zero LD receives C's absolute 1 um
step; drawn L=2 um then has zero effective length, which fails explicitly.
More importantly, an unset NSUB in levels 2/3/9 receives the absolute
1e-6 cm^-3 step, below intrinsic density. `mosXtemp.c` zeros the doping and
returns E_BADPARM; `sens_temp` ignores that failure and continues with stale
instance derivations. The port propagates the error rather than pretending
those partial-record values are a valid sensitivity. Give a valid NSUB or
filter out that parameter. The same temperature-callback limitation applies
where other perturbations make the device derivation invalid, in AC as in DC.
AC runs for levels 1/2/3/9 (with the walk's `IF_AC` entries: `cbd`/`cbs`,
the overlaps, `cj`/`cjsw`, and levels 2/3/9's IC voltages) and stays refused
for level 6, whose C device has no AC load at all.

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
was one slice per family rather than one generic helper. Every `z` level and
the Gummel-Poon `Q` and classic MOS levels now have one; a device type without
one (the code models) still refuses through `Device::sensitivity`'s `NotYetPorted`
default, naming its own `DEVparam`/`DEVtemperature`/`DEVload` reference, so a
deck that asks fails loudly.

## Divergences (explicit errors where C carries on)

* **AC, where the reset state is not reproduced** (see "The AC reset state"):
  MOS6 (no C AC load); a K card over more than two inductors
  (C's front end splits it into differently named pairs). A library caller
  that records an earlier transient without inheriting its actual final-load
  snapshot is refused, rather than silently using zero or DC capacitances.
* A BJT with `RC != 0`, a code model or an RF port in the circuit (see
  above). BJT `RC != 0` activates unported Kull quasi-saturation terms in the
  parameter walk; ordinary and single-sided IBE/IBC record replays are
  supported, including the paired-given restore side effect.
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
  `ikf`, an AC diode's bias-independent zero-bias linearization
  (`-gd/(1 + R gd)^2`), the MOS junction capacitances an earlier `.op`
  leaves (none after `.dc`), and the final load's capacitances after `.tran`
  for MOS1/2/3/9 (including after `.op` → `.tran` and across a later `.tf`).
  Snapshot layout validation and missing-snapshot errors are covered too;
  `tests/streamed_output.rs` checks stored/streamed snapshot equivalence and
  that failed runs never publish partial state.
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
  `sens_mesa`, `sens_mesa3`, `sens_mesa4`, `sens_mesa4_ts` and `sens_bjt`.
  The AC reset state (#102): `sens_ac_reset` (diode, S/W switches, B sources),
  `sens_ac_mutual` (K), `sens_ac_bjt`, `sens_ac_mos` (levels 1/2/3/9) and
  `sens_ac_fet` (JFET, MESFET, HFET2), `sens_ac_hetero` (JFET2, MESA levels
  2-4, HFET1 with `cds`/`kappa`) and `sens_ac_mos_op` (the MOS deck after
  `.op`, a batch fixture), under the nonlinear bound. Against the
  no-FMA oracle (`.options sparse`) every column is bit-identical except the
  MESFET's, HFET1's, HFET2's and MESA level 4's few-ulp residue, which a
  plain `.ac` of the same circuits shows too; it is visible only in columns
  that are themselves rounding noise (below 1e-12 on `sens_ac_hetero`,
  whose 117 larger values agree to 3e-13).
  `sens_ac_mos_tran` checks all three plots of `.op` → `.tran` → `.sens`
  for MOS1/2/3/9 with a varying bulk voltage, under the tighter linear
  sensitivity bound (filtered overlap/series/CJ columns). This bound catches
  substituting the operating point's capacitances for the final load's.
  Classic MOS adds `sens_mos1`, `sens_mos2`, `sens_mos3`, `sens_mos6` and
  `sens_mos9` under the same nonlinear bound.
* Opt-in live C (`NGSPICE_BIN=... cargo test --test c_sens_reference --
  --ignored`): DC linear and filtered decks, a hot behavioural/switch deck,
  diode decks with NaN, sidewall, breakdown and `.op` composition, and AC
  decks including `oct` and C's `lin` stepping, against `ngspice -b -r`:
  names **in order**, units, flags and values (NaN for NaN). The transient
  follow-on test covers MOS1/2/3/9, both polarities and trapezoidal/Gear
  integration with ordinary bias initialization, at the linear sensitivity
  bound; it also runs the new three-analysis fixture through the CLI.
