# BSIM3 and BSIM4 (`M`, M11, #90)

**Status: BSIM3 B3-0 (input, setup), B3-1 (temperature/setup), B3-2 (DC),
B3-3 (`capMod=3` charges and transient) and B3-4 (`b3acld.c` AC/`.dc`/`.pz` and
`b3noi.c` noise) are implemented for BSIM3v3.3.** Levels 8/49 select the
BSIM3v3.3 schema, validate explicit setters after model binning (#109) and
build a real device: `.op`, `.dc`, `.ac`, `.pz`, `.tf` and `.tran` solve
through `b3ld.c`/`b3acld.c`/`b3pzld.c`, `.noise` through `b3noi.c`, and `.disto`
treats the device as linear, as C does (no `DEVdisto`). The device runs C's
`BSIM3convTest` and `BSIM3trunc`. **Still refused:** `capMod` 0/1/2 (a different
charge equation set), every NQS mode (`nqsMod`/`acnqsMod`), `.sens` and older
BSIM3 versions. Each names its C file instead of substituting a law. BSIM3 is
a reimplementation, not a translation: it agrees with C to about 1e-9 at a
converged point but is not bit-exact.

**BSIM4 (4.8.3, `bsim4/`, levels 14/54) is ported and simulates every
analysis C supports for it:** `.op`, `.dc` (sources, temperature, instance
parameters), `.ac`, `.noise`, `.pz`, `.tf`, `.disto` (BSIM4 has no `DEVdisto`,
so it is linear there, as in C) and `.tran`, in every `capMod`, `mobMod`,
`rdsMod`, `rgateMod`, `rbodyMod`, `igcMod`/`igbMod`, `tempMod`, `mtrlMod`,
`gidlMod`, `dioMod`, `tnoiMod`/`fnoiMod` and NQS (`trnqsMod`/`acnqsMod`)
selection, bit-identical to an FMA-free build of the reference C (see
[BSIM4](#bsim4) below), and DC/AC `.sens` (`sens.rs`, see
[SENSITIVITY.md](SENSITIVITY.md)). The B3-0/B4-0
physical-unit audit of `Parameter::input_unit` is still partial.

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
  `gmbs`, `vth`, `vbs`, `vgs`, `vds`, `vdsat`, `ibs`, `ibd`, `gbs`, `gbd`) and
  the charge asks (`qg`, `qb`, `qd`, `cgg`..`cbs`, `capbd`, `capbs`), times the
  instance multiplicity. `cqg`/`cqb`/`cqd`, `qbs`/`qbd` and `qinv` are `b3ask.c`
  cases without an instance-table row in `b3.c`, so they are not askable.
- The device declares 28 state slots: the 19 DC slots plus C's charge slots
  (`qg`/`cqg`, `qb`/`cqb`, `qd`/`cqd`, `qbs`, `qbd`, `qinv`), each integrated
  charge immediately followed by its `NIintegrate` derivative slot.

Verification is opt-in: `c_bsim3_dc_reference` compares the pure core with C at
C's converged internal bias over 16 model/geometry/temperature/bias cases in
both polarities to 1e-9 relative, and `c_bsim3_device_reference` compares the
port's own `.op` solution with C's for eight decks (default and sub-micron
geometry, PMOS, RSH internal nodes, junction areas, MOBMOD 2, 100 °C) to 1e-6
on node voltages and source currents and 1e-9 on the asks.

#### The charge model (`b3ld.c` C-V) and small signal (`b3acld.c`, `b3noi.c`)

`charges::ChargeModel::evaluate` ports `b3ld.c`'s charge block at `capMod=3`:
the C-V preamble (`VbseffCV`, `CoxWL`, the `noff`/`voffcv` `Vgsteff`), the
charge-thickness block (`Vfbeff`, `Tcen`, `Coxeff`, `Qac0`/`Qsub0`,
`DeltaPhi`, `VgDP`, `VdseffCV`, `qgate`/`qbulk`, all three `xpart` partitions
and the nine `c*xb` derivatives), the bias-dependent `capMod >= 2` overlap
capacitances and the source/drain junction C-V. It reuses the DC pass's
intermediates (`DcEvaluation::cv`) rather than recomputing them. Two C quirks
are reproduced deliberately: the junction gate-sidewall zero-bias capacitance
reads `BSIM3unitLengthGateSidewallJctCap` (not the temperature-scaled field),
and `VdseffCV`'s gate/body derivatives are zeroed at `Vds = 0` while the drain
derivative is left alone.

`Device::stamp` evaluates that block on every DC load where the model is
`capMod=3` and QS (so `.op` state, asks and a later `.tran` start from C's
values) and on every transient load, then follows `b3ld.c`: the `gc*`
equivalent conductances from `ag0`, `NIintegrate` on `qb`/`qg`/`qd`, and the
`ceqq` companion currents. `assemble_small_signal` ports `b3acld.c` (the DC
conductances into `A`, the `j omega xc*` terms into `E`) and
`assemble_pole_zero` is its `s`-domain twin (`b3pzld.c` is `b3acld.c` with `s`
for `j omega`). `noise` ports `b3noi.c`.

Verification is opt-in and differential:

- `c_bsim3_charge_reference` compares `@m1[qg]`, `[qb]`, `[qd]` and
  `[cgg]`..`[cbs]` with C at C's converged bias over 19 cases in both
  polarities to 1e-9: all three `xpart` partitions, accumulation,
  subthreshold, triode, saturation, inverse mode, overlap-only cards, junction
  areas with and without temperature scaling and `mobmod=2`.
- `c_bsim3_small_signal_reference` compares `.dc` sweeps and `.ac` sweeps with
  C by variable name (1e-6 relative, 1e-12/1e-15 floors), and two `.tran`
  waveforms on a common physical grid (1e-3 relative, 1 uV/1 pA) — a slow gate
  charge through 1 MOhm and a drain step with the gate held high.
- `c_noise_reference::bsim3_noise_sources_match_c_batch_mode` compares every
  per-source noise column with C for `noiMod` 1..6, a series-resistance deck
  and a PMOS instance.

**Not available yet:** `capMod` 0/1/2, NQS, `.disto` and `.sens`. Those paths
fail with `NotYetPorted` naming the C files rather than loading zero. BSIM4 and
the other BSIM families are unchanged.

### Slice status

| Slice | State |
| --- | --- |
| B3-0 tables/schema | done for the generated interface facts and explicit setters; the **physical-unit audit is partial** (`Parameter::input_unit` still returns `None` for unaudited keywords) |
| B3-1 temperature/instance setup | done (`b3temp.c`, `b3set.c` defaults, `b3check.c` diagnostics) |
| B3-2 DC load | done, differentially verified |
| B3-3 charges and transient | done for `capMod=3`/QS; `capMod` 0/1/2 and NQS refused |
| B3-4 small signal / noise | done for `.dc`, `.ac`, `.pz`, `.tf`, `.noise` and `.disto` (linear, as C); `.sens` refused |
| B3-5 goldens and binning coverage | done: `m11_bsim3_{op,dc,ac,tran}` and `m11_bsim4_{op,dc,ac,pz,noise,tran,bin_dc}` under `xtask golden verify` (binning end to end in `m11_bsim4_bin_dc`) |
| B4-0 tables/schema | done |
| B4-1..B4-4 BSIM4 setup, temperature, checks, load, small signal, noise, pole-zero | done, bit-exact against the FMA-free C oracle ([BSIM4](#bsim4)) |
| B4 `.sens` | done (DC and AC), bit-exact on every deck C completes; see [SENSITIVITY.md](SENSITIVITY.md) |

### Next: what remains

1. **`capMod` 0, 1 and 2** (#190). `b3ld.c`'s three other charge branches, each with
   its own `Vfb`/`AbulkCV`/`VdsatCV` handling; every one is a different
   equation set, so each is a separate slice with its own differential test
   before it is wired.
2. **NQS** (`nqsMod`, `acnqsMod`, #191): the `qNode` unknown, `qcheq`/`qdef`/
   `qcdump` states, `gtau` and the `b3acld.c` NQS branch.
3. **Bit-level fidelity** (#193). BSIM3 is a reimplementation, so its arithmetic
   is not C's operation order: the `m11_bsim3_tran` fixture needs
   `reltol=1e-5` because at the default tolerance its step sequence drifts
   from C's. Translating `b3ld.c` as BSIM4 was translated would close it.
   (Done since the first draft of this list: `BSIM3convTest`, `BSIM3trunc`
   on qb/qg/qd — C does register `DEVtrunc` — C's `MODEINITTRAN`/
   `MODEINITPRED` voltage selection, and `.disto` as linear.)
4. **`.sens`** (#192). The `cktsgen.c` device tables for BSIM3 (a fifth family of
   tables); no `.sens` curve is C-differential yet.
5. **Physical-unit audit** of the remaining `input_unit() == None` keywords.

## BSIM4

### How it is ported

Unlike BSIM3 (a free reimplementation), BSIM4 is **translated statement by
statement** from `bsim4/` so that its arithmetic is C's: the BSIM4 sources
are licensed under the Educational Community License 2.0 with Berkeley's
terms of use, which allow modified redistribution with attribution (see
`NOTICE` and `LICENSE-ECL-2.0`). The translation keeps C's struct members:

- `cargo xtask bsim4-schema --source /path/to/ngspice [--check]` generates
  `parameters.rs` (897 model / 98 instance IFparm rows), `defaults_data.rs`
  (777 literal defaults) and `fields.rs`: every scalar member of
  `sBSIM4model` (1,824), `bsim4SizeDependParam` (224) and `sBSIM4instance`
  (370), named without the `BSIM4` prefix, with by-name accessors. No
  formula is generated.
- `set.rs` (`b4set.c`), `temp.rs` (`b4temp.c`), `check.rs` (`b4check.c`),
  `geo.rs` (`b4geo.c`), `load.rs` (`b4ld.c`, with `BSIM4polyDepletion`),
  `acld.rs`, `pzld.rs`, `noise.rs` (`b4noi.c`) and `cvtest.rs` follow their
  C files over those structs. `elements.rs` is `b4set.c`'s `TSTALLOC` order.
- `host.rs` is the slice of `CKTcircuit` the loads see: C's `CKTmode` bits,
  `CKTrhsOld`, `CKTstate0..2`, `NIintegrate`, matrix and RHS additions, on
  BSIM4-local node indices that alias exactly as C's node numbers do.
- `device.rs` implements those traits over the solver and maps the port's
  Newton phases to `CKTmode` (`MODEINITJCT/FIX/FLOAT/PRED/TRAN`, the `uic`
  `MODETRANOP|MODEUIC` load, `MODEINITSMSIG` for small-signal work).

### C behaviour the device reproduces

- **One struct per instance.** C's instance struct persists: each load reads
  members the previous load wrote (`BSIM4von` feeds `DEVfetlim`), and the
  convergence test (`b4cvtest.c`, run by `NIiter` in `MODEINITFIX` too, where
  an `off` instance is skipped) reads the last load's members and C's
  `CKTrhsOld`. The device keeps that struct and shares it with its
  `.dc @inst[param]` stand-ins.
- **Setup once, check once.** `BSIM4setup` runs once; later `BSIM4temp`
  passes (another temperature, `.dc temp`, an instance-parameter point) work
  on the structs as left and skip `BSIM4checkModel`, whose `static` model
  name is already set. A deck whose model only fails the check at a later
  temperature therefore simulates, as in C.
- **`noiseAnalGiven`.** With `tnoiMod=1`, a deck that holds a `.noise`
  analysis gets drain/source primes in every analysis (`ModelContext::
  noise_analysis`).
- **Small signal.** AC, noise, pole-zero and the asks run C's
  `MODEINITSMSIG` load at the iterate the last DC load used
  (`NewtonSolution::values`), which reproduces C's `CKTstate0`. The AC load
  is reassembled at every frequency (its `x* = cap * omega` terms and the
  NQS terms use C's `CKTomega`), and noise generators are described at the
  analysis frequency (`b4noi.c`'s flicker law and `tnoiMod=2`'s correlated
  gate noise, `NevalSrc2`).
- **`NIintegrate` outside transient.** NQS integrates in every load and the
  `uic` load integrates all charges, with the zero `CKTag` of a fresh
  circuit.
- **Pole-zero re-runs `BSIM4setup`.** `CKTpzSetup` calls `DEVpzSetup =
  BSIM4setup` on a fresh matrix, and its `else` branches alias every
  drain/source/gate/mid-gate prime it created back to the external node and
  reset `qNode`; a second run (the zero search of `.pz ... pz`) creates a
  node again where that alias is ground. The device declares and loads each
  search's topology (`MatrixSetup::pole_zero_setups`), records `b4pzld.c`'s
  conductance and `s` statements in order, and SPARSE leaves the dropped
  rows out of the determinant as C does.

### Deliberate differences

Tracked in #189 (cross-instance and cross-analysis C state), #186
(reporting warnings), #187 (SOA checks), #188 (speed) and #194 (versions
4.5-4.7).

- `BSIM4checkModel` runs for every instance at its first setup; C checks
  only the first instance of each model name in its list (and any model
  whose name is a prefix of an already-checked one is skipped). Warnings
  are kept on the device (`Prepared::warnings`), not printed.
- `DEVsoaCheck` (`b4soachk.c`) is not ported: it only prints warnings under
  `.options warn`, which the port does not accept.
- A swept `.dc @inst[param]` point re-runs `b4temp.c` from the first
  (checked) structs, not from the previous point's; C's results agree
  because `BSIM4temp` recomputes every member it writes.
- After a `.tran` in the same deck, C's `CKTag` keeps the last step's
  coefficients and a later NQS `.op`/`.ac` integrates with them; the port
  uses zero.
- `.sens ac` after a `.tran` is refused: C's AC reset load reads the
  `capbd`/`capbs` the final transient load left, which the port does not keep.

### Verification

- Offline: `tests/bsim4_schema.rs` (tables, setters, factory, fatal check)
  and `tests/bsim4_device.rs` (9 decks: OP with gate and body resistance,
  PMOS `rdsMod` sweep, AC, noise with `tnoiMod` 0/2 and `fnoiMod` 0/1,
  `.pz` with the setup re-run, an NQS CMOS inverter transient, a width
  sweep and a check-once temperature sweep), bit-exact on macOS arm64 and to
  1e-9 elsewhere.
- Opt-in: `c_bsim4_schema_reference`, `c_bsim4_setup_reference` (every
  numeric model member after `b4set.c`, 4,430 values over five cards),
  `c_bsim4_temp_reference` (6,314 model/instance values after `b4temp.c`,
  check messages) and `c_bsim4_device_reference` (a mode-by-analysis matrix
  against `NGSPICE_BIN`).
- During the port, ~250 decks (every selector above, both polarities,
  binning, multi-instance, `off`, `ic=`/`uic`, Gear, `.tf`, `.disto`,
  `.dc temp`, `.dc @m1[w|l|nf|m|sa|delvto]`) were compared with
  `fidelity-tools/scripts/ulpcmp.py` against the FMA-free oracle: all
  bit-exact, except decks C itself aborts (fatal checks), which fail with
  C's messages. The conformance scoreboard is unchanged.

### Notes for the next slice (`capMod` 0/1/2)

Measured from the C files while porting `capMod=3`, so the next session does not
rediscover them:

- `capMod=2` (~1791-1954) and `capMod=1` (~1629-1790) share the C-V preamble
  the port already computes (`VbseffCV`, `CoxWL`, the `noff`/`voffcv`
  `Vgsteff`), then differ only in the charge/VdsatCV handling and the
  partitioning. `capMod=0` (~1265-1620) is the outlier: it *overwrites*
  `Vbseff`/`dVbseff_dVb` (from `Vbs`/`Phis`, not the C-V preamble), uses
  `Vfb = pParam->vfbcv`, `Vth = Vfb + phi + k1ox sqrtPhis` and the DC `Vgst`
  rather than `Vgsteff`, and carries six partition/region sub-blocks.
- Three C quirks must be reproduced rather than "fixed":
  - `capMod=2`'s 40/60 partition computes `Csg = (T4 + T5*dVdseffCV_dVg)`
    **without** the `CoxWL` factor its 0/100 arm applies.
  - `capMod=2` stores `qinv = qinoi`, while `capMod=3` stores `-qinoi`.
  - `capMod=1`'s saturation arm uses `T0 = -0.4*CoxWL` for `xpart < 0.5`, the
    same constant as the linear arm but scaled by `Vgsteff`.
- The overlap capacitances have three variants, not two: `capMod=0` uses
  `cgdo = pParam->cgdo` with `qgdo = cgdo*vgd` (no `cgdl`/`ckappa` term),
  `capMod=1` uses the `sqrt(1 - 4 vgd / ckappa)` form, and `capMod >= 2` the
  `DELTA_1 = 0.02` form the port already implements.
- The junction C-V, the `gc*`/`NIintegrate` wiring, the small-signal load and
  the noise model are all independent of `capMod` and need no changes.

Verification for each new mode should extend
`tests/c_bsim3_charge_reference.rs` with a `capmod` field in its `Case` (the
harness already compares `qg`/`qb`/`qd` and `cgg`..`cbs` at C's converged
bias), and `c_bsim3_small_signal_reference.rs` with an `.ac` deck per mode.

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

Done; see [BSIM4](#bsim4).

## Guardrails

- C stays a read-only reference. BSIM3 is reimplemented; BSIM4 is translated
  under its ECL-2.0/Berkeley terms with the attribution in `NOTICE`. Each
  function names the C file it follows.
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
