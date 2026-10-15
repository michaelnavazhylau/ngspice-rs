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

## Not ported

`.disto` (`mesdset.c`, `mesdisto.c`) and `.sens` of a MESFET fail with
`NotYetPorted`; so do the MESA/HFET levels and the `nhfet`/`phfet` bases.

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

## MESA level 2 (`Z`, M10 H1a/H1b, #83)

`inpdomod.c` maps `nmf`/`pmf` levels to four macspice families, which share one
grammar and one 62-setter model table but not one channel:

| level | family | channel | state |
| --- | --- | --- | --- |
| 0/1 | MES | `mesload.c` | ported (wave 3) |
| 2 | MESA | `mesa1` (`mesaload.c`) | ported: every load but charge storage |
| 3/4 | MESA | `mesa3`/`mesa4`, `mesapz*` | H2 |
| 5 | HFET1 | `hfet1` | H3 |
| 6 | HFET2 | `hfet2x` | H4 |

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
differs only by round-off (about 1e-16 relative). `.sp` and `.tf` build on the
same assembly and run.

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
- `.noise`/`.disto`/`.sens` are #158: C's MESA entry point defines none of
  those three routines, so the reference binary runs them with MESA simply
  absent while the port refuses the run.

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
