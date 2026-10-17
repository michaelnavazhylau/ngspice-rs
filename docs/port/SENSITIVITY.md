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
| Z levels 3-4 (MESA) and 5-6 (HFET1/HFET2) | refused (`NotYetPorted`, #164) | refused |

The device hook is `Device::sensitivity(&ModelContext) ->
SpiceResult<Box<dyn DeviceSensitivity>>`; its default is an explicit
`NotYetPorted`, so a device whose C parameters are not reproduced is refused,
never left out of the list. The analysis stamps the stand-in devices a
description returns through `Circuit::load_device` (DC) and
`Circuit::assemble_small_signal_device` (AC); the diode stand-in is the
port's own `Diode` built from C's records.

### The `j` and `z` families (#164)

**Ported: JFET levels 1 and 2 and the MESFET (`z` level 1).** Each family
replays C's own sequence, the way the diode does, and each has a committed
C golden with every enumerated parameter:

| Family | `sgen` parameters | Fixture |
| --- | --- | --- |
| `jfet.c` | 20 model + 4 instance | `sens_jfet` (116 vectors, two models, both polarities) |
| `jfet2.c` | 36 model + 4 instance | `sens_jfet2` (148 vectors) |
| `mes.c` | 10 model + 1 instance in DC (12 model in AC: `cgs`/`cgd`) | `sens_mesfet` (90 vectors) |

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

**MESA level 2 is ported** (`sens_mesa`, 69 columns: `mesa.c`'s 61 model and 8
instance parameters, verified at `compare::SENSITIVITY_CANCELLED` - see below).
**Not ported: MESA levels 3-4 and HFET1/HFET2.** The tables, records and
replay are mechanical (the `mesa.c`/`hfet1.c`/`hfet2.c` enum tables have 62-70
entries each), but two C behaviours have to be reproduced exactly first, and
both were measured and left open:

- **`MESAphib`/`MESAphib1`'s asymmetry.** `mesamparam.c` stores
  `value * CHARGE` while `mesamask.c` returns the stored joules unchanged, so
  C's perturbation (`+value*1e-6` in "eV") and its restore (`ask * CHARGE`)
  both corrupt the stored barrier. `v(z2:phib)` comes out as exactly 0 in the
  reference binary while the port's rebuilt load answers `1.8e25` - the
  values are not comparable until C's own sequence is understood.
- **Operating-point branches.** Any MESA deck with a *responsive* node (a
  drain load or a gate resistor rather than a stiff supply) has more than one
  fixed point and C lands on a different one from the port - the same class
  documented in [MESFET.md](MESFET.md). Measured on a level-2 card at
  `vgs = 0.6` behind a 1 k drain load, C's device is cut off
  (`i(vdd) = -6.1e-7 A`) while the port conducts 3.1 mA. A `.sens` golden
  needs the stiff-supply topology of the `m10_mesa_op` fixture *and* a
  responsive node, which only the internal `#gate` node provides.

### What MESA levels 3-4 and HFET1/HFET2 still need

Both were implemented against C's tables during the #164 work and then
*withheld*, because their rows do not verify:

- **MESA level 4** (measured on a level-4 card alone, `catch`-free):
  `teta1` differs by 100% (`-2.4e-4` vs `~0`), `epsi` by 0.98% (`152.30` vs
  `153.79`), `gamma` by 4.0% and `xtm0` by 1.1%. `.dc`/`.tran`/`.op` for the
  same card agree with C to 1e-9, so this is a *derivation* difference that
  only the small-signal-by-perturbation walk exposes - the level-3/4 channel's
  dependence on `tEta`/`epsi`/`gamma` is the place to look.
- **HFET1**: `js1s` differs by 2.8x, `vt2`/`l`/`w` by 0.1-100%, again with the
  ordinary loads exact. `hfettemp.c` also multiplies the **stored** threshold
  by the model type on every `DEVtemperature` call, so a p-channel device's
  `vt0` alternates sign across the walk; any HFET1 fixture must be n-channel
  until that is reproduced.

Neither is blocked on the framework: the tables, records and replay are the
same as the four families above. They are held back because shipping
sensitivity numbers that are 1-100% off on a subset of columns is worse than
the explicit refusal.

### Two bugs this work found and fixed

- **`phib` on the card is in electron-volts.** `mesamparam.c` and
  `hfetmpar.c` store `value * CHARGE`; the port stored the card value raw, so
  `.model ... phib=0.1` was silently ~1e19 times too large. Fixed for MESA
  (`phib`, `phib1`) and HFET1 (`phib`), with the schema default moved to the eV
  figure (0.5) so the derived default is unchanged. Verified against the
  reference binary: `phib=0.1` now answers `i(vgs) = 1.212700e-03` and
  `i(vids) = 5.403663e-03` A on both.
- **MESA's level is a `double`.** `mesadefs.h` stores `MESAlevel` as a `double`
  and `mesaload.c`/`mesatemp.c` test `== 2`, `== 3`, `== 4`, so a value between
  them matches *no* branch and the channel loads nothing. The port kept it in a
  `u8`; `.sens` perturbs the field, so `level`'s column was wrong until the
  representation was corrected.

Until those land the refusal stays, naming `cktsens.c`/`cktsgen.c` and the
device's `DEVparam`/`DEVtemperature`/`DEVload`.

### The `j` and `z` families: the original gap note (#164)

C's modern `.sens` does **not** use the SPICE2 adjoint `*sload.c` routines, so a
device whose entry point sets `DEVsenSetup = NULL` is still enumerated:
`cktsens.c` walks every real parameter of every instance and model
(`cktsgen.c`'s `sgen_init`/`sgen_next`), perturbs it through `DEVparam`/
`DEVmodParam`, calls `DEVtemperature`, reloads the device and solves. Measured
with the reference binary on `z1 2 3 0 zm l=1u w=20u`, `.model zm nmf(level=2)`,
`vgs=0`, `vds=0.5` and `.sens v(2)`, it exits 0 and its `sens1` plot carries 61
`z1:*` vectors (`z1:ag`, `z1:alpha`, `z1:astar`, `z1:beta`, `z1:cas`, ...).
`j1` is refused by the port the same way for the same reason.

That work is **not done**: the port refuses the whole `j`/`z` designator family
with `SensitivityFamily` having no variant for it. What it needs, in this
order, is exactly what [`nonlinear/sens.rs`](../src/devices/nonlinear/sens.rs)
does for the diode but per family:

1. `mesamparam.c`/`mesaparam.c` (and `jfet.c`/`jfet2.c`, `mes/`) `IFparm`
   tables in C's order, filtered to the entries `set_param()` accepts;
2. the `*setup.c` default and *given*-flag side effects, which persist from one
   perturbed parameter to the next (`mesasetup.c` alone fills ~30 fields);
3. `*temp.c`'s derived values;
4. a load that builds the port's own device from the records, so the stamp is
   the perturbed one.

The four families share the card grammar but not the channel or the tables, so
this is one slice per family rather than one generic helper. Until it lands the
refusal names `cktsens.c`, `cktsgen.c` and the device's
`DEVparam`/`DEVtemperature`/`DEVload`, so a deck that asks fails loudly.

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
