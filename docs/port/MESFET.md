# MESFET level 1, Statz (#83, M10 wave 3)

`Z` instances with an `nmf`/`pmf` model of level 0 or 1 are built by
`devices::mesfet` (registry status `bounded`). The behaviour is read from the
C reference, never linked: `parser/inp2z.c`, `parser/inpdomod.c`,
`devices/mes/mes.c` (parameter tables), `mesparam.c`, `mesmpar.c`,
`messetup.c`, `mestemp.c`, `mesload.c`, `mesacl.c`, `mespzld.c`,
`mestrunc.c`, `mesgetic.c`, `mesask.c` and `mesnoise.c`. No full SPICE parity
is claimed.

## Front end

`Zname nd ng ns model [off] [ic=vds[,vgs]] [area=] [m=] [icvds=] [icvgs=]`
(`src/netlist/parser/mesfet.rs`). INP2Z reads exactly three terminals and
then the model token. The `ic` vector fills `icvds`, then `icvgs`
(`mesparam.c`'s `MES_IC` fallthrough); more than two fields are a parse error
and later setters of a component win. Unknown setters (MES has no
`temp`/`dtemp`), extra terminals and non-scalar values are explicit gaps
(`NotYetPorted`, `inp2z.c`).

An **unlabeled leading value** (`z1 d g s zm 2`) is refused. INP2Z only
reaches its `area` setter for a non-MES model, and `MESApTable`,
`HFETApTable` and `HFET2pTable` have no `area`: C reports "no such parameter
on this device or parameter is missing" and stops the run (checked against the
reference binary). For a MES model the value is captured by `PARSECALL` and
silently discarded, so accepting it here would either guess the model or
ignore input. The error asks for `area=<value>` (MES) or `m=<value>`.

The netlist writer round-trips the card and the `nmf`/`pmf` model families,
including the `nmf`/`pmf` tail flags.

## Model selection

`ModelFamily::{Nmf, Pmf}`. As `inpdomod.c`, the first `level` selects the C
device: 0 or 1 is MES (this device); 2, 3 and 4 are MESA, 5 HFET1 and 6
HFET2, which fail with `NotYetPorted` naming `mesa/`, `hfet1/` and `hfet2/`;
any other level fails naming `inpdomod.c` (C rejects it too). The `nhfet` and
`phfet` bases, which C maps through the same switch, remain unsupported model
types. The tail flags `nmf`/`pmf` set the polarity in card order starting
from the base (`.model m nmf(pmf)` is a PMF), as `MES_MOD_NMF`/`MES_MOD_PMF`
do.

## Parameters

Model setters (`MESmPTable`, defaults from `messetup.c`): `VTO`/`VT0`
(-2 V, last spelling wins), `ALPHA` (2 /V), `BETA` (2.5e-3 A/V²), `LAMBDA`
(0), `B` (0.3 /V), `RD`/`RS` (0 ohm), `CGS`/`CGD` (0 F), `PB` (1 V), `IS`
(1e-14 A), `FC` (0.5), `KF` (0), `AF` (1). Instance setters: `AREA` (1), `M`
(1), `ICVDS`, `ICVGS`, `IC`, `OFF`. Unknown setters are rejected before any
node is created.

Domains where C would divide by zero or misbehave: `ALPHA` positive (C
divides by it), `B`, `BETA`, `LAMBDA`, `RD`, `RS`, `CGS`, `CGD` nonnegative,
`PB` and `IS` positive. `FC` only feeds `mestemp.c`'s `depl_cap` and
`f1`-`f3`, which `mesload.c` never reads; it is validated as finite and has
no effect, exactly as in C.

## Equations

- **No temperature dependence.** `mestemp.c` only derives the series
  conductances and `vcrit = Vt0 ln(Vt0 / (sqrt(2) IS))`; every voltage uses
  C's fixed `CONSTvt0 = k * 300.15 K / q`, whatever `.options temp` says.
  Only the noise generators see the circuit temperature (`NevalSrc`).
- **Channel** (`mesload.c`): with `vgst = vgs - VTO` (normal mode, `vds >=
  0`), `I = BETA AREA (1 + LAMBDA vds) vgst^2 / (1 + B vgst)` times the
  cubic saturation factor `1 - (1 - ALPHA vds / 3)^3` below `vds = 3 / ALPHA`
  (C's polynomial in place of Statz' `tanh`), zero at `vgst <= 0`; the
  inverse mode exchanges the junctions. `gm`/`gds` are C's partials (exact
  derivatives; checked by finite differences in every region).
- **Gate diodes**: `IS * AREA` with `Vt0`, C's cubic continuation below
  `-3 Vt0`, plus `gmin` per junction.
- **Gate charge** (`qggnew`): the Statz charge with `vcap = 1 / ALPHA`, the
  0.2 V smoothing and the `vmax = 0.5 V` linear continuation, scaled by area.
  In transient it is incremental, `qgs = qgs1 + (q(vgs, vgd) - q(vgs1, vgd)
  + q(vgs, vgd1) - q(vgs1, vgd1)) / 2` (and symmetrically `qgd`), integrated
  by the companion (`NIintegrate`), and both charges enter truncation control
  (`mestrunc.c`). Every DC load stores the total charge in both slots, as
  `mesload.c`'s `MODEINITTRAN` load writes it into `CKTstate1`: `CKTterr`'s
  charge tolerance scales with `|q|`, so this offset fixes the first steps
  (the C and Rust step sequences of `m10_mesfet_tran` coincide). With
  `PB <= 0.5 V` a forward-biased gate makes `qggnew` take the root of a
  negative number (C: NaN); the port reports a numerical error in transient
  and AC and stores zero in DC loads, which nothing integrates.
- **Series resistance**: `RD`/`RS` (conductance `AREA / R`, times `M`) create
  the internal nodes `<name>#source` then `<name>#drain`, in `messetup.c`'s
  order, as `NodeKind::Internal` rows.
- **AC / pole-zero** (`mesacl.c`, `mespzld.c`): the bias conductances plus
  `j omega` (or `s`) times `qggnew`'s gate-source and gate-drain
  capacitances, each a two-terminal element.
- **Noise** (`mesnoise.c`, family order between `dio` and `mos1`): `_rd`,
  `_rs` thermal (`AREA M / R`), `_id` channel thermal `4 k T (2/3) M |gm|`
  and `_1overf` flicker `M KF |cd|^AF / f` between d' and s', where `cd` is
  C's `MEScd` (channel minus gate-drain diode current), then the instance
  total; all at the circuit temperature.

## Newton start and limiting

Shared with the JFET (`jfet::limit_junctions`): `MODEINITJCT` starts at
`vgs = vgd = -1` (zero for `off`); later loads apply `DEVpnjlim` (`Vt0`,
`vcrit` from the model's unscaled IS) and then `DEVfetlim` against `VTO` to
both junction voltages. Under `uic` the initial load evaluates at
`type * IC`, unset components from the external terminals (`mesgetic.c`).
Divergences, as for the JFET: no `MODEINITPRED` extrapolation or bypass, and
any limited gate step keeps the load nonconvergent.

**`off`.** As `jfetload.c`, `mesload.c` starts `icheck` at 1 and never clears
it for a held `off` load, so C cannot leave `MODEINITFIX`: its gmin and
source stepping fail and it falls back to the transient operating point,
which then agrees with the port. The port uses the shared held-load rule of
the other nonlinear devices. No golden covers `off`; the opt-in C test
covers it in a `uic` transient and Rust tests in DC.

## Deliberate divergences

- **Exact transient Newton matrix.** The port stamps the cross derivatives of
  the incremental charges (`d qgs / d vgd`, `d qgd / d vgs`, as
  `devices::jfet2` does) where `mesload.c` stamps each charge as a
  two-terminal capacitance at `qggnew`'s partials. The equations and their
  roots are C's; only the iteration differs.
- **Pole-zero multiplicity.** `mespzld.c` writes `m * ggd + ggs`,
  `m * gdpr + gds + ggd` and `m * gspr + gds + gm + ggs` on the diagonal (an
  operator-precedence slip), so C's pole-zero matrix is inconsistent for
  `m != 1`. The port's pole-zero load is its AC load with every term scaled
  by `m`; both agree for `m = 1`.
- **Leading value** refused, `ALPHA <= 0` and negative `B` rejected (above).

## Observations

`@z[...]` asks follow `mesask.c`: `cd`, `cg`, `is` are terminal currents in
the device's **normalized** polarity (C stores them before the `type`
factor), including the charge currents in transient; `gm`, `gds`, `ggs`,
`ggd` and the gate-drain current `cgd` (times `m`) and `vgs`, `vgd`
(normalized) are evaluated at the converged solution; `area` reports
`AREA * M`; `icvds`, `icvgs` when given. In transient `ggs`, `ggd` and `cgd`
are refused (C adds the charge companion). C answers no `m` ask (`MES_M` is
missing from `MESask`), nor does the port; `qgs`, `qgd`, `cqgs`, `cqgd` and
`p` are explicit errors. The port labels `cd`/`cg`/`cgd` as currents (C's
front end guesses `capacitance` from the names). `.dc
@z1[area|m|icvds|icvgs]` sweeps rebuild the instance (MES has no
temperature setup to re-derive).

### `qgs`/`cqgs`/`qgd`/`cqgd` are deferred (state-backed asks)

`mesaask.c`, `hfetask.c` and `hfet2ask.c` answer those four out of
`CKTstate0`, the charge and its derivative that the **load** accumulated. The
port's observation path is deliberately stateless - `devices/observe.rs`
re-loads a disposed copy of each device from the solved point and accepts no
state or history ("Loads are disposable; no observation accepts state or
advances history"). Reproducing C therefore needs more than a formula:

- the accepted state has to reach the observation, which means threading the
driver's per-point state through `Circuit::observe_real`/`ObservationRequest`
(only `record_small_signal_state` writes `Circuit::carried_state` today, and
that is the `MODEINITSMSIG` vector, not `CKTstate0` after a transient point);
- the observing load has to run with the same integration coefficients as the
solved point, otherwise the port would recompute a charge from the bias and
report it *as if* it were C's;
- `.dc` sweeps make it order-dependent: C accumulates the charges and its
`CKTstate0` at point *k* carries point *k-1*'s accepted charge, so the answer
depends on how the previous point was accepted, not only on the current bias.

Deferred rather than approximated: the asks stay `NotYetPorted` naming the C
routine, so a deck that reads them fails loudly instead of getting a
recomputed charge. Every `z` level exports them (`mesask.c` for 0/1,
`mesaask.c` for 2-4, `hfetask.c`/`hfet2ask.c` for 5/6); the port refuses them
for the MES, MESA and HFET families alike.

## `.sens` (#164)

`mes.c`'s tables: 10 model parameters in a DC run, 12 in AC (`vto` is the `IOPR` alias of `vt0`,
`gd`/`gs`/`depl_cap`/`vcrit` are ask-only, `cgs`/`cgd` are `IF_AC`, and
`kf`/`af` are settable but **not askable** - `mesmask.c` has no case for them,
so `sens_getp` fails and C skips them) and one DC instance parameter, `area`
(`MES_M` is `IOPUR`, so a MES `.sens` has no `z1_m` column). `mestemp.c` has
no given-flag branches and MES has no instance temperature, so the replay has
no temperature step. `sens_mesfet` verifies 90 columns over two models and
both polarities.

## Not ported

`.disto` (`mesdset.c`, `mesdisto.c`) fails with `NotYetPorted`; so do `.sens`
of MESA levels 3-4 and of HFET1/HFET2 (see
[SENSITIVITY.md](SENSITIVITY.md)), and the `nhfet`/`phfet` bases. MESA level 2
`.sens` is ported (`sens_mesa`).

## Evidence

- `tests/mesfet.rs`: grammar and gaps (including the leading value), level
  selection and validation, restated Statz regions of both polarities,
  inverse-mode symmetry, internal-node order, finite-difference DC Jacobians
  (Newton load and small-signal `A`) over a bias grid of both polarities with
  RD/RS, area and `m`, AC capacitances against a restated `qggnew`,
  observations, noise generators, explicit distortion/sensitivity refusal,
  transient gate charge and the `PB` error, `@z1[area]` sweeps, writer round
  trip; unit tests in `devices::mesfet` for the channel partials,
  drain/source antisymmetry and `qggnew`'s partials.
- C goldens (`cargo xtask golden verify`): `m10_mesfet_dc`, `m10_mesfet_ac`,
  `m10_mesfet_tran`; see [VERIFICATION.md](VERIFICATION.md#m10-mesfet-level-1-83).
- Opt-in `tests/c_mesfet_reference.rs` (live C): saved asks of both
  polarities, `.dc @z1[area] @z1[m]`, `.pz` roots, `.noise` spectral
  densities of every generator, and a `uic` transient with an `off` device.

## MESA levels 3 and 4 (`Z`, M10 H2, #168)

Both levels share everything with level 2 - the card, the junctions, the load,
the charge storage, `mesaacl.c`, `mesapzl.c`, `mesaask.c` - and differ in three
`mesatemp.c` derivations and the channel:

| | level 2 (`mesa1`) | level 3 (`mesa2`) | level 4 (`mesa3`) |
| --- | --- | --- | --- |
| threshold model | one depth term `vpo` | two-population `vpou`/`vpod`, four regions | same as level 3 |
| channel | `ns` from `1/(ND D (1-sqrt1))` | `nsa`/`nsb` split with `ca`/`cb` | `nmax`/`gamma` knee |
| `n0` depth | `d` | `du` | `d`, and with the `/2` `mesatemp.c` gives only level 4 |
| `gchi0` | `CHARGE W/L` | `× tMu` | `× tMu` |
| permittivity | `EPSILONGAAS` | `EPSILONGAAS` | the model's `epsi` |
| `cf` | `0.5 EPSILONGAAS W` | same | `0.5 epsi W` |
| capacitance | `2/3 cgc` | `2/3 cgc` | `2/3 cgc` with `cgc` from `cas`/`cbs` |

Two C details the port had to read literally: `mesatemp.c`'s `n0` has the `/2`
**only** for level 4 (the sibling line for the other levels does not), and
`mesa2`'s `nsa` selection reads `vpod`/`vpou`/`vpo` in four regions, each with
its own `ca`/`delnsavgte`.

### Level 4's swept small-signal path is refused (a C defect)

`mesaacl.c` and `mesapzl.c` build their conductances from `gm0`/`gm1`/`gm2`,
`gds0` and `delidgch0`/`delidvds0`/`delidvds1`, which `mesa1` and `mesa2` store
in the instance and **`mesa3` never writes**. For a level-4 device those fields
keep their zeroed initial values, so C's AC load has no transconductance and no
output conductance at all. Measured on the `m10_mesa_l4_op` card, which carries
-3.600846442234158e-3 A in DC, the reference binary prints 1.07e-8 A at 1 kHz
and the same at 1 MHz - a frequency-independent artefact, not a response.

The port therefore refuses **`.ac`, `.sp` and `.pz`** for a level-4 device with
a message naming the unset fields, the same choice TX0 made for `y`/TXL. The
guard is gated on the presence of a bias state, which is what distinguishes a
swept small-signal assembly from the other two paths that build a matrix:

- **`.dc` sweep works.** The port assembles the small-signal system to resolve
a swept source (C resolves that from its DC Jacobian) and needs only the
correct linearisation, which the port computes.
- **`.tf` works.** C's `CKTtf` linearises the converged DC Jacobian instead of
loading `mesaacl.c`, so the level-4 channel's real `gm`/`gds` are in the
system. On a common-source card with `rd=1k`, `vds=2 V`, `vgs=0.4 V` the
reference binary answers 1.818568640594385e-01 for the gain, 1.222280009536926e+03
for the input impedance and 1.818568640594385e+02 for the output impedance;
the port agrees to 2e-9 relative
(`tests/mesa.rs::the_level_four_transfer_function_matches_the_reference_binary`).
Only the *swept* path would have to reproduce C's zeroed fields.
- **Transient works** and is where `cas`/`cbs` reach the load.

### Fixtures

`m10_mesa_l3_{op,dc,ac}` (the upstream `tests/mesa/mesa21.cir` card, a sweep
across the `vpod`/`vpo` regions and a small-signal sweep - level 3 *does* store
the AC intermediates, so its `.ac` matches C) and `m10_mesa_l4_{op,dc,tran}`
(the `cas`/`cbs`/`epsi` model, the `nmax`/`gamma` knee and the transient, which
is where those capacitances reach the load since the level's AC is refused).

One deck is deliberately *not* a fixture: a **mixed-polarity level-4 `.op`**
(the configuration `m10_mesa_h2`'s first draft used). That circuit has two
roots and C's continuation picks the other one, so a golden would pin a
continuation accident. Measured with the reference binary on `vdd=3 V`,
`vgg=0.5 V` with `rd=rs=31`, its n-channel device settles at
`@z1[vgs] = 4.287953e-01`, `@z1[gm] = 2.024911e-14` (channel cut off),
`@z1[cd] = 9.567193e-08` and `@z1[cg] = 2.296831e-03` A - a genuine fixed
point, because the 0.0712 V across `RS = 31` carries exactly that gate
current.

The port lands on the **conducting** root instead, and that root is C's own:
the same n-channel card alone, with `vgg` swept up to 0.5 V, is answered by the
reference binary as `i(vdd) = -3.785019e-3` A and `i(vgg) = -2.768870e-4` A,
the values the port reaches in the two-device deck to the last printed digit.
The p-channel device agrees either way (`i(vdd2) = 6.408350e-2` A).
`tests/mesa.rs::the_mixed_polarity_level_four_operating_point_is_the_conducting_root`
retains this as a reproducer: it pins the port's branch and cross-checks it
against the single-device sweep that C agrees with bit for bit.

## MESA level 2 (`Z`, M10 H1a/H1b, #83)

`inpdomod.c` maps `nmf`/`pmf` levels to four macspice families, which share one
grammar and one 62-setter model table but not one channel:

| level | family | channel | state |
| --- | --- | --- | --- |
| 0/1 | MES | `mesload.c` | ported (wave 3) |
| 2 | MESA | `mesa1` (`mesaload.c`) | ported: every load but charge storage; `.sens` included (#164) |
| 3 | MESA | `mesa2` (`mesaload.c`) | ported: DC, transient (H2) |
| 4 | MESA | `mesa3` (`mesaload.c`) | ported: DC, transient, `.tf`; `.ac`/`.sp`/`.pz` and `.sens` refused (H2, #164) |
| 5 | HFET1 | `hfeta` (`hfetload.c`) | ported (H3) |
| 6 | HFET2 | `hfeta2` (`hfet2load.c`) | ported (H4) |

Level 2 is `src/devices/hfet/{mod,mesa}.rs`. `mod.rs` owns the union of the
four families' setters (the card accepts `l`, `w`, `m`, `td`, `ts`, `dtemp`,
`temp`, and an unlabeled leading value is an error because MESA defines no
`area`), the `pmf` -> NMF coercion `mesasetup.c` reports as "Only nmf model
type supported, set to nmf", and the per-family refusal of everything else.
`mesa.rs` ports `mesatemp.c`'s derivations, the `mesa1` channel, the DC load
with all 30 matrix entries and 5 RHS terms, the `mesaask.c` table and the
`mesaacl.c` small-signal load. C's `CKTtemp`/`CKTnomTemp` are kelvin while the
port's `ModelContext` carries Celsius: the default `ts`/`td` and the
temperature deltas are converted with `+ CONSTCtoK`, which is the difference
between `vts = 2.33 mV`/`ggrwl = 0.15 S` and the correct `6.5e-6`.

`mesaacl.c` turned out to need no H1b work: the `m10_mesa_ac` fixture verifies
the small-signal sweep against the reference binary, and single-device probes
agree with it bit for bit over 1 kHz-10 GHz while the two-device fixture
differs only by round-off (about 1e-16 relative). `.sp` builds on the same
assembly and runs for levels 2 and 3.

## HFET2 (level 6, `Z`, M10 H4, #169)

The smallest of the three lineage copies: `hfet2/` has 33 model setters (`cf`,
`d1`/`d2`, `del`, `delta`, `deltad`, `di`, `epsi`, `eta`/`eta1`/`eta2`, `gamma`,
`ggr`, `js`, `klambda`/`kmu`/`knmax`/`kvto`, `lambda`, `m`/`mc`, `mu`, `n`,
`nmax`, `p`, `rd`/`rs`/`rdi`/`rsi`, `sigma0`, `vs`, `vsigma`, `vsigmat`,
`vt0`/`vt1`/`vt2`), one instance temperature `temp`, 13 state slots and only
two conditional internal nodes.

Where it differs from the other two levels:

- **Only `#source`/`#drain`.** The model table has no `rg`, so there is no
  `#gate` node, and there is no `ri`/`rf` prime-prime pair. `ri`/`rf` are not
  setters at all here - C's table spells the channel's series pair `rdi`/`rsi`.
- **`pnjlim` *and* `fetlim`.** `HFET2load` limits the junctions with
  `DEVpnjlim` before `DEVfetlim` (HFET1 has only the latter), so `icheck` is
  live and a strongly forward-biased gate takes a path the other levels do not
  have. `vcrit` comes from `hfet2temp.c`'s fixed `1e-11` constant rather than a
  junction current.
- **A single-exponential gate junction** (`JS`, `GGR`) with no `gatemod`, no
  `a1`/`a2` corrections and no `cds` charge; the transient folds the
  integration companion straight into `ggs`/`ggd` (HFET1 keeps it in
  `ggspp`/`ggdpp`).
- **The `eta2`/`d2` sub-threshold branch** is C's `nsc` compound, taken only
  when both setters are given (`hfeta2` checks both, while `hfet2temp.c` gates
  `N02` on `eta2` alone).
- **Mixed thresholds.** `hfet2temp.c` scales `vto` into `TVTO` but leaves
  `HFET2_VT1`/`VT2` on the *raw* threshold, and `hfeta2`'s `nsc` branch mixes
  the two (`exp((vgt + TVTO - VT2)/(ETA2*vt))`). The port reproduces that as
  written.

### Deliberate divergences and quirk notes

- `hfet2temp.c` computes `N01` and reads it nowhere; the port does not carry
  it.
- Three of C's matrix-entry identifiers are mangled by a bad substitution
  (`HFET2sourcePriHFET2ourcePrimePtr`); they still map to
  `(sourcePrime, sourcePrime)`, `(sourcePrime, source)` and
  `(drainPrime, sourcePrime)` in `hfet2setup.c` and `hfet2load.c` alike, which
  is what the port stamps.
- `.noise`/`.disto` contribute nothing (both entry points are NULL) and
  `.sens` is refused, as for the rest of the family (#164).
- No level-6 deck exists in the C tree (`tests/hfet/` covers only HFET1), so
  the five `m10_hfet2_*` fixtures are the port's own decks with C goldens.

## HFET1 (level 5, `Z`, M10 H3, #83)

`hfet1/` is the third copy of the macspice lineage, with its own model table
(`vt0`, `lambda`, `rd`/`rs`/`rg`/`rdi`/`rsi`/`rgs`/`rgd`/`ri`/`rf`, `eta`, `m`,
`mc`, `gamma`, `sigma0`/`vsigmat`/`vsigma`, `mu`, `di`, `delta`, `vs`, `nmax`,
`deltad`, `js1d`/`js2d`/`js1s`/`js2s`, `m1d`…`m2s`, `epsi`, `p`, `cm3`, `a1`,
`a2`, `mv1`, `kappa`, `delf`, `fgds`, `tf`, `cds`, `phib`, `talpha`, `mt1`,
`mt2`, `ck1`, `ck2`, `cm1`, `cm2`, `astar`, `eta1`, `d1`, `vt1`, `eta2`, `d2`,
`vt2`, `ggr`, `del`, `gatemod`, `klambda`/`kmu`/`kvto`) and one instance
temperature, `temp`.

Where it differs from MESA, and what the port must not share:

- **Two polarities.** Every node difference is multiplied by the type (`+1`
  for `nhfet`, `-1` for `phfet`) before the equations and the loaded residual is
  multiplied back; `hfettemp.c` stores `threshold *= type`. The card base
  (`phfet`) sets the polarity and a later `nhfet`/`phfet` flag overrides it, as
  C applies setters in card order (`tests/hfet1.rs` pins all three cases
  against the reference binary: -2.10801e-04 A, -1.14799e-05 A, -2.10801e-04 A).
- **No resistance temperature scaling.** `hfettemp.c` turns `rd`/`rs`/`rg`/`ri`/
  `rf` straight into conductances; there is no `rtc1`/`rtc2`.
- **Two gate models.** `gatemod == 0` (the default) uses `leak()`, a
  two-exponential junction with an optional series resistance and C's `diode()`
  approximation; `gatemod != 0` replaces both gate currents with thermionic
  expressions and adds the `gmg`/`gmd` cross conductances to the load, to the
  `cghat`/`cdhat` convergence test and to the AC matrix.
- **A `cds` charge.** `qds`/`cqds` accumulate `cds*vds`, integrate like the gate
  charges and fold into `gds`/`cd`; `hfettrunc.c` truncates only `qgs`/`qgd`.
- **`a1`/`a2` corrections.** C gates them on `gatemod != 0` *and* on the
  coefficients. This port's first version applied them unconditionally: the
  reference binary answers 5.707950675392426e-4 A for the card below without
  `gatemod` and 5.707819384864532e-4 A with it, and `tests/hfet1.rs` pins both.

### C quirks reproduced deliberately

- `hfeta`'s first argument is `vds > 0 ? vgs : vgd` *after* `vds` has been made
  positive, so it is `vgs` in inverse mode too - a slip against MESA's
  `inverse ? vgd : vgs`. The `m10_hfet1_op`/`m10_hfet1_tran` goldens pin it.
- `gmg`/`gmd` are only written when `gatemod != 0`; C's `else` branch nulls
  local pointers and leaves the caller's zeros, so the port returns the
  caller's junction values unchanged in that case.
- With `gatemod != 0` and an off channel (`nsm < 1e-38` returns with
  `vsate = 0`), the default `ck2 = cm2 = 0` make `vds/vmax` and `vdse/vkneet`
  `0/0`, and every `gatemod` term NaN. C carries that NaN into the matrix and
  recovers through `DEVfetlim` on the next iteration (the reference binary
  answers a `gatemod=1` deck through dynamic gmin stepping); the port refuses
  to store non-finite state and takes the mathematical limit instead - no
  channel, no gate-modulated current. The fixed point is unchanged whenever
  C's own answer is finite, which the `m10_hfet1_ac` golden checks.
- The `type` string setter (`type=NHFET`) has no scalar-schema representation;
  use the `nhfet`/`phfet` flag or the card base, which C resolves the same way.

### The upstream `tests/hfet/inverter.cir`

`tests/hfet/id_vgs.cir` is the `m10_hfet1_dc` fixture (exact to 1e-15). The
inverter deck is a DCFL chain whose DC operating point has two fixed points:
the reference binary lands on `v(3) = v(4) = -0.2755 V` and the port on
`v(3) = 1.9557 V`, `v(4) = 0.2063 V`. Both are genuine solutions of the same
equations - feeding the port's values back to C as a `.nodeset` reproduces the
port's answer to all printed digits - so this is a continuation-path
difference, not a device discrepancy. The transient fixtures therefore use
single-device decks (with an independent second device in the DC/AC fixtures),
and the project's transient fixtures for this family remain
`m10_hfet1_tran`/`m10_mesa_tran`.

### The `cds` charge keeps its derivative (found by fixing the fixture)

`m10_hfet1_tran` used to place its gate pulse on a node no device touched, so
the gate was never excited and the fixture could not fail. With the pulse
connected, the port disagreed with C by up to 5% of the drain current **during
the source edges** - and only there, which is what a charge-history defect looks
like. The cause was `HFETAload`'s fixed `cds` charge: its companion current
(`cd -= cqgd; cd += cqds`) was right, but the port stored `0` in the derivative
slot `HFETAcqds` instead of that companion value. Trapezoidal order 2 reads
`state1[ccap]` back (`NIintegrate`'s `- state1[ccap]*ag1`), so every step after
the first was missing one term - invisible at `cds=0` (the port is then
bit-identical to C), invisible in DC, and wrong at every edge.

`tests/hfet1.rs::the_drain_source_charge_keeps_its_derivative_in_the_state_history`
pins it: the reference binary answers -7.699785266e-3 A at the end of the rising
edge and -4.934298603e-3 A at the end of the falling edge, the short-companion
version lands 1.5% and 2.8% away, and the test's bounds are 30 to 50 times
tighter than that. With the fix, the three recaptured gate-pulse fixtures
(`m10_hfet1_tran`, `m10_hfet2_tran`, `m10_mesa_l4_tran`) verify with a worst
error of 0.001 of the bound.

### The resistance paths (`rd`/`rs`, `rg`, `ri`/`rf`, `rdi`/`rsi`)

`mesasetup.c` creates `#source` for `rs`, `#drain` for `rd`, `#gate` for `rg`,
`#gs` for `ri` and `#gd` for `rf`, in that order, and makes each node its
unfurled predecessor when its resistance is zero, so the prime-prime pair falls
back to the prime pair. `rdi`/`rsi` create no node at all: `mesatemp.c` folds
them into `mesa1`'s `RS`/`RD` series terms, which appear in `rt = RS + RD`,
`f = sqrt(1 + 2 a RS)`, `d = 1 + a RS + f`, `ddevgte` and `delisatabeta` -
the last two plus `delgchgchi = 1/(1 + gchi rt)^2` are the parts a derivative
comparison exposes and a converged DC solution does not: `mesaacl.c` matched C
to 1.78% on `rdi`/`rsi` alone until that denominator was right.

The reference binary's rawfile **omits** `#source`/`#drain` (its `outitf.c`
skip list is a mainline-device list: `#source`, `#drain`, `#collector`,
`#emitter`, `#base`, `#internal`), which is why an early reading of
`rd=1e6 rs=1e6` looked like a nodaless resistance. `show z1` disproves it:
`dprimenode`/`sprimenode` are real nodes, and the current collapses to
`0.5/(rd + rs) = 2.5e-7 A` because the 1 MOhm pair starves the channel.
`xtask/src/verify.rs` projects the port's extra columns away for exactly that
list, while keeping `#gate`, `#gs` and `#gd` compared.

Three fixtures cover the paths (`m10_mesa_res_op`, `m10_mesa_res_dc`,
`m10_mesa_res_ac`), and a 30-deck randomized comparison against the reference
binary (mixed `rd`/`rs`/`rg`/`ri`/`rf`/`rdi`/`rsi`, `dtemp` and
`rtc1`/`rtc2`, `m` and four channel geometries) matches to round-off.

### Boundaries (explicit errors, not silence)

- An unknown *card* parameter is an error in C ("Model issue on line ..."), so
  the port rejects it rather than ignoring it. C also accepts *instance*
  parameters on a `.model` card (`inpgmod.c` records them in the model's
  `defaults` list, so `.model nm nmf(dtemp=25)` shifts every instance); the
  port rejects those, a general parser gap rather than a MESA one (#161);
the reference decks' `.print vids#branch` spelling is a second one (#162).
### `.noise`, `.disto` and `.sens`

`mesainit.c` leaves `DEVnoise`, `DEVdisto` and `DEVsenSetup` NULL (as do
`hfet1init.c` and `hfet2init.c`), and the three behave differently:

- **`.noise`**: C runs the analysis and the card adds nothing. `mes/` has
  `mesnoise.c`; MESA and both HFET levels have none. The port returns
  `DeviceNoise::Noiseless`, and the measured reference behaviour is a run whose
  spectrum is the rest of the circuit's (a 1 kOhm load: 4.071371529487329e-9
  V/sqrt(Hz) input-referred), not an error.
- **`.disto`**: same - no routine, no contribution, all harmonics zero. The
  port returns `DeviceDistortion::Linear`.
- **`.sens`**: `DEVsenSetup` is a red herring. ngspice's `.sens` does not use
  the adjoint `*sload.c` routines at all; `cktsens.c`/`cktsgen.c` enumerate
  every device's parameter tables through `DEVparam`/`DEVload`, so the
  reference binary reports 61 `z1:*` parameters for a MESA card and exits 0.
  The port still refuses, because `SensitivityFamily` has no `z`/`j` variants -
  a gap that covers the JFET too, tracked as #164.

### Charge storage and the transient load (#156)

`mesaload.c`'s charge block is *not* the JFET's: MESA has no closed-form gate
charge. C accumulates it,

```c
state0[QGS] = state1[QGS] + capgs*(vgspp - vgs1);
```

from the accepted charge and the prime-prime voltage, and only a transient load
runs `NIintegrate` on it (`mesatrunc.c` truncates both pairs). `.dc` still
accumulates - that is what `@z1[qgs]` reports mid-sweep - while `.op` skips the
block entirely. The port reproduces the accumulation, the `MODEINITTRAN`
`state1[QGS] = capgs*vgspp` seeding (and the copy of the accumulated charge
into `state1` before integration) and the companion; the convergence test then
compares `cghat` against the *integrated* `cg`/`cd`, as C does.

`m10_mesa_tran` (a gate pulse through every resistance path, 244 accepted
steps) verifies under `compare::TRAN`. A scratch comparison of a 100-step pulse
showed the port landing on C's own accepted timepoints and agreeing to 4e-7
relative on the drain current, and a second deck with `ri`/`rf`/`rdi`/`rsi`
showing a 7e-8-relative step difference after a breakpoint, which the common
grid absorbs.

### Pole-zero (#157)

`mesapzl.c` is `mesaacl.c` with `s` in place of `j omega` in the `Ptr + 1`
entries: the port assembles the same dynamic terms and lets its pole-zero
analysis multiply them by `s`, so `assemble_pole_zero` delegates to
`assemble_small_signal`. C's lambda blend reads `CKTomega`, which the pole-zero
driver never sets, so the blend frequency is 0 in both. `m10_mesa_pz` (a
common-source stage with a current drive at the gate, as for `pz_mos1`) matches
C's three poles and two zeros exactly.

### The asks C reads from state

`mesaask.c` reports `qgs`/`cqgs`/`qgd`/`cqgd` straight out of `CKTstate0`, and
those are accumulated across accepted points: a `.dc` sweep prints the previous
point's charge. The port's ask hook sees only the converged solution, so it
refuses those four keys explicitly rather than recomputing them from the bias
and presenting the result as C's.

### The `eps0` constant

The `ns` term above was the first bug; the slice's remaining ~1.2e-6
drain-current difference, which held `m10_mesa_op`/`m10_mesa_dc` out of
`cargo xtask golden verify`, was the second and came from a constant.
`mesaload.c` spells `EPSILONGAAS` as `12.244*8.85418e-12` and `mesasetup.c`
defaults `MESAepsi` to the same expression, while the port used the modern
`eps0` (`8.854_187_817e-12`), 8.8e-7 larger. An `fprintf` probe of `mesa1`'s
intermediates in a scratch copy of the C tree showed the signature directly:
every quantity proportional to eps (`beta`, `n0`, `isatb0`, `cf`) was high by
exactly 8.83e-7 and `vpo`, which is inversely proportional, was low by the
same. It was never a convergence artefact - `.options reltol=1e-9 vntol=1e-12
abstol=1e-15` leaves both simulators' answers bit-identical - and the node
voltages and the `mesaask.c` junctions agreed to the printed precision
throughout, which is why the probe, not the comparison, was needed to find it.
Both fixtures now verify against the reference binary like any other.

### A single-letter `d=` is an assignment

`d` is both a model-type flag and a model parameter: `.model m nmf(d=0.24u)`
sets `mesa1`'s device depth, while a bare `d` stays the flag. The
reference binary answers 4.162441e-04 A for the doubled depth against
1.926577e-03 A at the default. `parser/model.rs`'s `scalar_assignment`
refused the first because it tested the token's spelling alone; it now
requires a missing `=` for a flag, as C's `INPpSetP` does.
