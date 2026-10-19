# BSIM3 and BSIM4 (`M`, M11, #90)

**Status: B3-0 input, setup and the DC operating point are implemented.
Charges, AC, noise, pole-zero and BSIM4 are not.**
Levels 8/49 select the BSIM3v3.3 schema and validate explicit input setters,
including after model binning (#109). The factory builds a real device: levels 8/49 solve `.op` through the DC load,
while `.dc`, `.ac`, `.tran`, `.noise`, `.pz`, `.disto` and `.sens` fail with
`NotYetPorted` naming the missing C files. Older BSIM3 versions and all other
BSIM families remain explicit refusals. **B3-0 is not complete:** the
physical-unit audit is partial.

This guide is the implementation plan for **milestone M11**. #90 moved off M10 so
that M10 closes on the device library defined in [M10.md](M10.md); the issue
keeps its acceptance criteria and lands here.

## Scope

1. **BSIM3v3.3 first** - `src/spicelib/devices/bsim3/`, which is what
   `level=8`/`49` with `version=3.3` or with no version string selects.
2. **BSIM4.8+ second** - `src/spicelib/devices/bsim4/`, the default for
   `level=14`/`54`; the issue's "BSIM4.7+" is covered by it being newer than
   `bsim4v7/`.

Explicit non-goals: BSIM-SOI (`bsim3soi_{fd,dd,pd}`, `bsimsoi`), HiSIM2/HiSIM-HV,
BSIM1/BSIM2 (`bsim1/`, `bsim2/`), and the older `bsim3v0`/`bsim3v1`/`bsim3v32`
and `bsim4v5`/`bsim4v6`/`bsim4v7` variants. A `version=` string that selects one
of those stays an explicit `NotYetPorted` naming its C directory - never a
silently substituted model.

## Size, measured

| C files | Lines |
| --- | --- |
| `bsim3/b3ld.c` (the DC/transient load) | 3,191 |
| `bsim3/b3temp.c` | 894 |
| `bsim3/b3set.c` | 1,205 |
| `bsim3/b3check.c` | 396 |
| `bsim3/b3mpar.c` + `b3par.c` (param plumbing) | 1,919 |
| `bsim3/b3acld.c`, `b3pzld.c`, `b3noi.c` | 1,191 |
| `bsim4v7/` replacement set (for scale) | 15,959 |
| `bsim4/b4ld.c` | 5,623 |
| `bsim4/b4mpar.c` | 3,685 |
| `bsim4/b4temp.c` | 2,408 |
| `bsim4/b4set.c` | 2,779 |
| `bsim4/b4check.c` | 935 |

BSIM3 `b3.c` has **429 model entries** (425 distinct IDs, four redundant
`vtho` aliases) and **49 total instance entries**: **19 settable** and **30
ask-only**. The earlier count of 19 referred only to instance inputs, not the
whole IFparm array. BSIM4 `b4.c` has 897 model and 44 settable instance entries.
Input types, aliases, table order and read/write access all matter; a table
entry alone is not a supported equation or observable.

## Why not reuse the `mos` shell?

`devices::mos` (`src/devices/mos.rs`) exists because MOS1/2/3/6/9 share ngspice's
classic MOS interface: four terminals, a normalized device frame, junction
geometry and a `mosXnoi.c`. BSIM3/BSIM4 share none of it:

- their own state layout (`CKTstate0` slots per `b3ld.c`/`b4ld.c`, not the
  classic `qgs`/`qgd`/`qbs`/`qbd`/`cd`/`cgm` shape);
- a different node set (drain/source primes plus the `b3trunc.c`/`b4trunc.c` LTE
  slots, and BSIM4's `rbodyMod`/`rbodymod` internal bulk node);
- `b3check.c`/`b4check.c` diagnostics that alter or reject cards;
- `capMod`/`mobMod`/`rdsMod`/`igcMod` mode selection inside the load, not a
  fixed equation set.

So BSIM gets its own module tree (`src/devices/bsim3/`, `src/devices/bsim4/`)
behind the same `Device` trait, exactly as `devices::hfet` does for the `z`
family. The `mos` shell stays untouched.

## Slices

Each slice is independently landable and leaves the branch green; BSIM3 and
BSIM4 repeat the same shape.

### B3-0 · Tables and schema (no load)

- Generate the model/instance schema from `b3.c`'s `IFparm` arrays rather than
  hand-writing 429 entries: a small generator in `xtask` (a checked-in
  intermediate, not a build dependency) emitting a `schema` table plus the
  `(keyword, C field, unit)` map the setters need. C's table order is part of the
  contract (it is the `.sens` order and the `show all` order), so the generator
  must preserve it.
- Wire `levels()` to accept 8/49 for the BSIM3 factory and dispatch it; keep the
  unsupported `version=` strings explicit.
- Deliverable: `.model` cards from real PDKs parse and validate with C's
  defaults, checks and `b3check.c` diagnostics; `.op` still fails explicitly.

#### Implemented foundation (not the full B3-0 deliverable)

- `cargo xtask bsim3-schema --source /path/to/ngspice [--check]` reads six
  clean upstream files (`b3.c`, `bsim3def.h`, `b3mpar.c`, `b3par.c`, `b3set.c`,
  `b3temp.c`). It emits
  only interface facts into `src/devices/bsim3/parameters.rs`: keyword, canonical
  alias, numeric/symbolic C ID, setter destination, given bit, input type and
  access. It preserves all 429/49 entries in C order, records the source Git
  revision and rejects unfamiliar macros/types or ambiguous destinations.
  No C implementation, descriptions, headers or formulas are copied; Cargo
  and ordinary tests need no C source tree or generator invocation.
- `devices::bsim3::{model_parameters, ModelParameters::instance_parameters}`
  project ordered explicit setters. Duplicates and aliases retain last-set
  precedence and source locations; unset fields remain absent, not fake zeros
  or setup defaults. Model-level instance defaults are prepended in C's reverse
  order; model-level `m` is discarded. Physical units are provided only for an
  explicitly reviewed subset; `input_unit() == None` means unaudited.
- `b3mpar.c` TNOM-to-kelvin and the distinct NPEAK/NGATE conversion thresholds
  are reproduced, as are checked `floor(x + 0.5)` integer setters. `b3par.c`
  scales lengths once and areas left-to-right twice (avoiding a spurious
  `scale^2` overflow), and IC expands in VBS/VGS/VDS fallthrough order.
- The winnow parser/writer preserve bare dotted MOS VERSION strings. The first
  VERSION selects the directory (`inpdomod.c`/`INPfindVer`), while its setter
  retains the last value. `3.0*`, `3.1*`, `3.2*` fail naming the correct older
  directory rather than selecting 3.3. Quoted/expression versions remain refused;
  the C probes reject those forms too. The M grammar additionally recognizes
  `nqsmod`, `acnqsmod`, `geo`, `delvto`, `mulu0`; this enables input validation,
  not a simulation backend for them.
- `conformance/parser/bsim3_schema.cir` is a hand-authored PDK-shaped input
  probe with NMOS bins and PMOS, **not a foundry PDK or simulation golden**.
  Production tests exercise every model keyword and instance input, ask-only
  rejection, alias/flag/default/IC precedence, overflow, version round-trips,
  real bin selection and atomic factory refusal. Opt-in
  `c_bsim3_schema_reference` checks explicit fields against C on levels 8/49.

#### Setup implementation

`ModelParameters::model_defaults` (`b3set.c`), `ModelDefaults::at_temperature`
(`b3temp.c`) and `ModelParameters::setup_instance` (instance defaults, effective
geometry, size-dependent parameters and `b3check.c`) expose the setup stage:

- 376 literal model defaults are generated facts; ~46 dependent defaults
  (TNOM→Kelvin, TOXM/DSUB/L-W copies, COX/CF/CGDO/CGSO/CGBO formulas, polarity
  VTH0/U0/NOIA/NOIB/NOIC, MOBMOD-selected UC/UC1) are reimplemented in
  `defaults.rs` with C's given-bit guards. Ungiven zero-allocated fields such as
  K1/K2/VFB stay distinguishable via `ModelDefaults::is_given`.
- `b3temp.c`'s model pass reproduces junction-potential clamps, saturation
  current and junction-capacitance temperature scaling, VTcrit/factor1, and the
  PN-junction potential clamps; its per-size pass reproduces the 83 generated
  binning rows (L/W/reciprocal-area coefficients, `binUnit` scaling), the
  mobility/temperature adjustments, U0 > 1 downscale, VSAT/RDS temperature
  terms, Φ/Vbi/Xdep0/CDep0/Ldeb/ACDE derivations, K1/K2 either explicit or
  derived from doping, VBSC/VFB/VTH0/VFBZB, K1OX/K2OX, θ0VB0/θRout, and the
  COV overlap capacitances. C's recomputation of `tconst` with the surviving
  `sqrt(1 + nlx/leff)` temporary rather than `leffCV²` is reproduced and
  documented as a quirk.
- `b3check.c` reports every fatal and warning check in C order (`Diagnostic`),
  including the A2/RDSW/CGDO/CGSO/CGBO/CALCACM resets. Fatal checks fail setup;
  warnings remain data. `Pb/Ps < W` warnings appear for default zero perimeters,
  exactly as C emits them.
- ACM geometry (`acm != 0`) is refused explicitly: `ACM_SourceDrainResistances`
  is shared, unported C code. Nothing is approximated by the non-ACM path.

`b3check.c` resets live in C's single `BSIM3model` struct, so a reset made while
setting up one instance changes the model every later instance sees.
`ModelParameters::model_setup` returns a mutable `ModelSetup` that holds that
state; `ModelSetup::setup_instance` applies CALCACM and CGDO/CGSO/CGBO resets in
C order and keeps them, so a model with two instances warns once and then uses
the corrected values. `ModelParameters::setup_instance` is the single-instance
convenience over a fresh `ModelSetup`.

#### DC core (`b3ld.c`), validated but not yet wired

`devices::bsim3::dc` ports the bias-dependent physics of `b3ld.c` as a pure
evaluation: `DcModel::evaluate(Bias, gmin)` returns the drain current and its
`gm`/`gds`/`gmbs`, the substrate current and its derivatives, the source/drain
junction currents and conductances, and C's observables (`von` for `@m1[vth]`,
`Vdseff`, `Vdsat`, `Abulk`, `ueff`, `Rds`, `Vgsteff`). It reproduces:

- source/drain junction diodes with the `ijth`/`vjsm` breakdown branch and gmin,
- `Vbseff`/`Phis`/`Xdep`, the short/narrow-channel `Vth` terms (`dvt0/1/2`,
  `dvt0w/1w/2w`, `k1ox`/`k2ox`, `k3`/`k3b`, temperature terms, DIBL shift),
- subthreshold `n`, poly-gate depletion, the three `Vgsteff` regimes,
  `Weff`/`Rds` bias dependence, `Abulk`, all three `mobMod` mobility branches,
  the `a1`/`a2` lambda split for `Vdsat`, `Vdseff`, `Vasat`, `VACLM`, `VADIBL`,
  `VASCBE`, `Ids`/`gm`/`gds`/`gmbs` and the substrate current.

The core deliberately omits what belongs to the solver loop: voltage limiting
(`DEVfetlim`, `DEVpnjlim`, `DEVlimvds`), bypass and convergence bookkeeping,
matrix stamps, state slots and internal-node conductances. Charges (`capMod`),
NQS, AC, noise and pole-zero are not evaluated, so `@m1[cbs]`/`@m1[cbd]`
(capacitances in C) and every charge ask remain unavailable.

#### The device (`device.rs`)

`devices::bsim3::device::Bsim3` wires that core into the solver:

- `b3set.c`/`b3temp.c` setup is cached per (temperature, nominal temperature),
  as C re-runs `BSIM3temp` on changes; the effective geometry, junction
  currents and diagnostics come from the setup stage.
- `stamp` reproduces `b3ld.c`'s start conditions (`off`, `ic=`), its
  `DEVfetlim`/`DEVlimvds`/`DEVpnjlim` limiting phases and its `line900` RHS and
  matrix terms for the forward and inverse modes, including the substrate
  current and both junction diodes. RSH/NRD/NRS create C's internal
  `drain`/`source` prime nodes.
- `observation_operating` serves C's `b3ask.c` DC keywords (`id`, `gm`, `gds`,
  `gmbs`, `vth`, `vbs`, `vgs`, `vds`, `vdsat`, `ibs`, `ibd`, `gbs`, `gbd`)
  times the instance multiplicity. Charge/NQS/noise asks remain unavailable.
- The device declares 19 DC state slots. C's `BSIM3numStates` charge slots will
  be added with the charge port; nothing outside the device reads them.

Verification is opt-in: `c_bsim3_dc_reference` compares the pure core with C at
C's converged internal bias over 16 model/geometry/temperature/bias cases in
both polarities to 1e-9 relative, and `c_bsim3_device_reference` compares the
port's own `.op` solution with C's for eight decks (default and sub-micron
geometry, PMOS, RSH internal nodes, junction areas, MOBMOD 2, 100 °C) to 1e-6
on node voltages and source currents and 1e-9 on the asks.

**Not available yet:** `.dc` (the port's sweep setup assembles the
bias-linearized system, so it needs the charge/AC load), `.ac`, `.tran`,
`.noise`, `.pz`, `.disto`, `.sens`, `.tf`, and every charge ask. Those paths
fail with `NotYetPorted` naming `b3acld.c`/`b3noi.c`/`b3pzld.c` rather than
loading zero. BSIM4 and every B3-1..B3-5 equation slice are unchanged.

### Next: the charge model (resume here)

The charge slice is scoped and must keep the same verification discipline as
the DC core. C locations, in evaluation order:

1. `b3ld.c` charge preamble (~1540-1600): `VbseffCV`/`dVbseffCV_dVb`,
   `CoxWL = cox*weffCV*leffCV`, and the CV-specific `Vgsteff`
   (`noff`/`voffcv`) with its three regimes.
2. `capMod == 3` (the C default) at ~1955-2246: CTM `Vfbeff`, `Tcen`,
   `Coxeff`, `Qac0`/`Qsub0`, `DeltaPhi`, `VgDP`, `VdseffCV`, `qgate`/`qbulk`,
   the 0/100, 40/60 and 50/50 `xpart` source partitions, and the nine
   capacitance derivatives `cggb..cbsb`. `capMod` 0/1/2 stay explicit
   refusals until each is ported.
3. Overlap caps/charges (~2497-2620): bias-dependent `cgdo`/`cgso` for
   capMod 2/3 (`ckappa`, `DELTA_1 = 0.02`), `cgbo` from the model.
4. Junction CV (~2255-2430): `czbd/czbs` geometry (area, sidewall split by
   `weff`), `capbs`/`capbd` and `qbs`/`qbd` with the `MJ`/`MJSW`/`MJSWG`
   branches and `PhiB`/`PhiBSW`/`PhiBSWG`.
5. `b3acld.c`: with QS (`nqsMod == 0`) the bias-linearized system is the DC
   conductance stamp plus `j*omega` times the `xc*` matrix built from
   `cggb..cbsb`, `cgdo`/`cgso`, `cgbo`, `capbd`/`capbs`; the same matrix
   serves `.pz` (`b3pzld.c`). `.dc` unblocks as soon as
   `assemble_small_signal` exists.
6. `b3noi.c` then needs `qinv` plus the `ueff`/`rds`/`gm` values already on
   `DcEvaluation`; `b3trunc.c` names the charge slots for transient.

Verification must stay C-differential, not tolerance-loosened: compare
`@m1[qg]`, `@m1[qb]`, `@m1[qd]`, `@m1[cgg]`..`@m1[cbs]`, `@m1[capbd]`,
`@m1[capbs]` at fixed bias (as the DC core test does), then an `.ac` sweep and
a `.tran` RC charging curve against C, and only then add a golden fixture.
Noise must match C's `onoise` per source. `DcEvaluation` already carries every
intermediate the charge preamble needs (`vgs_eff`, `vth`, `vbseff`, `abulk0`,
`n`, `vgst`, `phis`, ...); expose them rather than recomputing.

### B3-1 · Temperature and instance setup

`b3temp.c` (894 lines) plus `b3set.c`'s geometry and *given*-flag handling:
BSIM3's `L`/`W`/junction geometry, `binUnit`, `m`, NQS/ACM choices, and the temperature
scaling of `VTH0`, `K1`, `K2`, `UA`, `UB`, `UC`, mobility and series resistance.
Binning (#109) resolves `l`/`w` before this runs; the port's resolver already
implements C's inclusive 1 nm edges and last-declared-match rule. `NF`,
`SA`/`SB`/`SD` and instance `dtemp` are not BSIM3v3.3 table entries; do not
accidentally import BSIM4 inputs into the BSIM3 schema.

### B3-2 · DC load (BSIM3 core)

`b3ld.c` without `capMod`: `b3set.c`'s mode checks, the `mobMod` variants,
`vth`, `Abulk`, series resistance/ACM and `rdsw`, `Vgsteff`/`Vdseff`/`DIBL`/`CLM`,
subthreshold, and the Newton stamps. Validate against C by name with the usual
relative **and** near-zero absolute tolerances, over PDK-shaped cards.

### B3-3 · Charges and transient

`capMod=0/1/2/3` charge partitioning, `XPART`, the `b3trunc.c` LTE slots, and
the `.tran` companion path. This is where the HFET1 lesson applies: every
charge **derivative** slot must survive into the accepted history (see
[MESFET.md](MESFET.md#hfet1-level-5-z-m10-h3-83)), because trapezoidal order 2
reads it back and a zeroed slot is invisible in DC but wrong in every edge.

### B3-4 · Small signal, noise, pole-zero

`b3acld.c`, `b3pzld.c`, `b3noi.c`. `.disto` and `.sens` stay explicit refusals
unless a slice takes them (`.sens` for BSIM is the same `cktsgen.c` walk as
#164 and would be a fifth family of tables).

### B3-5 · Goldens and binning coverage

PDK-style fixtures: at least one deck per `capMod`/`mobMod`/`rdsMod` mode the
port claims, a binned `.<n>` family end to end (binning resolution -> factory ->
`.op`/`.dc`/`.ac`/`.tran`), and a `b3check.c` diagnostic deck. Capture with
`cargo xtask golden capture`, review every value, keep existing goldens
untouched.

### B4-x · BSIM4

Repeat B3-0..B3-5 against `bsim4/` (897 model entries, `rdsMod`/`rbodyMod`/
`igcMod`/`cgeomod` modes, `b4geo.c`, `b4soachk.c`). BSIM4's extra bulk-resistance
nodes and gate-current paths are the parts that do not lift from BSIM3.

## Guardrails

- C stays a read-only reference (`B3TERMS_OF_USE`/`B4TERMS_OF_USE` are in the C
  tree; do not copy code or headers). Behaviour is reimplemented in Rust and
  each function names the C file it follows, as everywhere else in the port.
- Unsupported modes, cards and `version=` strings fail with `NotYetPorted`
  naming the C file - never a substituted model, never a silently wider range.
- Generated tables are reviewed artefacts, not build output: they are committed
  with the C revision they were generated from, the way goldens record the
  binary that produced them.
- Ordinary tests need no C; PDK-deck comparisons remain opt-in via `NGSPICE_BIN`.
- Do not loosen tolerances or recapture existing goldens to make BSIM pass.

## First step

B3-0, because it is where the size is: if 429 schema entries plus the `(keyword,
field, unit)` map cannot be generated and reviewed, the rest of the plan needs
re-scoping before any load is written.
