# JFET levels 1 and 2 (#82, M10 slices 1 and 6)

`J` instances with an `njf`/`pjf` model of level 1 are built by
`devices::jfet` (registry status `bounded`). The behaviour is read from the C
reference, never linked: `parser/inp2j.c`, `devices/jfet/jfet.c` (parameter
tables), `jfetset.c`, `jfettemp.c`, `jfetload.c`, `jfetacld.c`, `jfetpzld.c`,
`jfettrun.c`, `jfetask.c` and `jfetic.c`. No full SPICE parity is claimed.

## Front end

`Jname nd ng ns model [area] [off] [ic=vds[,vgs]] [area=] [m=] [ic-vds=]
[ic-vgs=] [temp=] [dtemp=]` (`src/netlist/parser/jfet.rs`). INP2J reads exactly
three terminals and then the model token (no terminal scan); an unlabeled
leading value is the area, applied after the named setters as in INP2D/INP2Q.
The `ic` vector fills `ic-vds`, then `ic-vgs` (`jfetpar.c`'s `JFET_IC`
fallthrough), more than two fields are a parse error, and later setters of a
component win. Unknown setters, extra terminals and non-scalar values are
explicit gaps (`NotYetPorted`, `inp2j.c`). The netlist writer round-trips the
card and the `njf`/`pjf` model families, including the `njf`/`pjf` tail flags.

## Model selection

`ModelFamily::{Njf, Pjf}`. As `inpdomod.c`, the first `level` selects the
backend: 0 or 1 is this device, 2 is the Parker-Skellern device
([below](#level-2-parker-skellern)), any other level fails with
`NotYetPorted` naming `inpdomod.c` (C itself rejects it). The tail flags `njf`/`pjf` set the
polarity in card order starting from the base (`.model m njf(pjf)` is a PJF),
as `JFET_MOD_NJF`/`JFET_MOD_PJF` do.

## Parameters

Model setters (`JFETmPTable`, defaults from `jfetset.c`): `VTO`/`VT0` (-2 V,
last spelling wins), `BETA` (1e-4 A/V²), `LAMBDA` (0), `RD`/`RS` (0 ohm),
`CGS`/`CGD` (0 F), `PB` (1 V), `IS` (1e-14 A), `N` (1), `FC` (0.5), `B` (1),
`TNOM`, `TCV` (0), `VTOTC`, `BEX` (0), `BETATCE`, `XTI` (only applied when
given, as in `jfettemp.c`), `EG` (1.11 eV), and the noise inputs `KF` (0),
`AF` (1), `NLEV` (2), `GDSNOI` (1), accepted although `.noise` is not ported.
Instance setters: `AREA` (1), `M` (1), `TEMP`, `DTEMP` (0), `IC-VDS`,
`IC-VGS`, `OFF`. Unknown setters are rejected before any node is created.

Fail-closed deviations from C's warn-and-continue behaviour: `FC > 0.95`
(C clamps it to 0.95 with a warning) and any nonfinite or nonpositive derived
value (PB after temperature scaling, IS, a `bFac` division by `PB = VTO`) are
errors. `BETA`, `LAMBDA`, `RD`, `RS`, `CGS`, `CGD` must be nonnegative.

## Equations

- **Temperature** (`jfettemp.c`): `TEMP`, else the circuit temperature plus
  `DTEMP`; `IS(T) = IS exp((T/TNOM - 1) EG / (N Vt)) (T/TNOM)^XTI`;
  `PB(T)` and the CGS/CGD factors by C's band-gap laws; `VTO(T)` by `VTOTC`
  (when given) else `-TCV`; `BETA(T)` by `1.01^(BETATCE dT)` (when given) else
  `(T/TNOM)^BEX`. Every load re-derives these from the analysis temperature,
  so `.options temp`, `.dc temp` and instance `TEMP`/`DTEMP` all apply.
- **Channel** (`jfetload.c`, Sydney University mod): normal and inverse mode,
  cutoff/linear/saturation, `bFac = (1 - B) / (PB - VTO)` with the model's
  (unscaled) PB and VTO, `LAMBDA`. The partials `gm`, `gds` are C's (exact
  derivatives; checked by finite differences in every region).
- **Gate diodes**: `IS * AREA` with `N Vt`, C's cubic continuation below
  `-3 N Vt`, plus `gmin` per junction.
- **Gate charge**: depletion charge with grading 1/2, linear continuation
  above `FC * PB(T)` (`f1`, `f2`, `f3`), scaled by area. In transient both
  charges are integrated by the companion (`NIintegrate`) and both enter
  truncation control (`jfettrun.c`); the stored charges are per device, as in
  C, and the multiplicity scales every stamp.
- **Series resistance**: `RD`/`RS` (conductance `AREA / R`, times `M`) create
  the internal nodes `<name>#source` then `<name>#drain`, in `jfetset.c`'s
  order, as `NodeKind::Internal` rows; C's default save set omits them.
- **AC / pole-zero** (`jfetacld.c`, `jfetpzld.c`): the bias conductances plus
  `j omega` (or `s`) times the gate capacitances.

## Newton start and limiting

`MODEINITJCT` starts at `vgs = vgd = -1` (zero for `off`); later loads apply
`DEVpnjlim` (`vt = kT/q`, `JFETvcrit` from the per-device IS) and then
`DEVfetlim` against `VTO(T)` to both junction voltages
([`devices::limiting`](../../src/devices/limiting.rs)). Under `uic` the single
initial load evaluates at `type * IC` (unset components from the external
terminals, `jfetic.c`), before the `off` rule, as C orders it. Divergences,
shared with MOS1: no `MODEINITPRED` extrapolation or bypass, and any limited
gate step keeps the load nonconvergent.

**`off` divergence.** `jfetload.c` starts `icheck` at 1 and never clears it
for a held `off` load, so every `MODEINITFIX` load of an `off` JFET is
nonconvergent: C cannot leave `MODEINITFIX`, its gmin and source stepping fail
the same way, and the operating point fails (a deck with an `off` JFET prints
`Dynamic gmin stepping failed` / `source stepping failed` and garbage values
from C). The port uses the shared `Limiter::holds_off`/`test_held` rule of the
other nonlinear devices, so an `off` JFET converges. No golden covers `off`.

## Observations

`@j[...]` asks follow `jfetask.c`: `id`, `ig`, `is` are terminal currents in
the device's **normalized** polarity (C stores them before the `type` factor,
so a PJF reports minus the physical current), including the charge currents
in transient; `gm`, `gds`, `ggs`, `ggd`, `igd` (times `m`) and `vgs`, `vgd`
(normalized) are evaluated at the converged solution through
`Device::observation_operating`; `area` reports `AREA * M` as C does; `m`,
`temp`, `dtemp`, `ic-vds`, `ic-vgs`. In transient `ggs`, `ggd` and `igd` are
refused (C adds the charge companion to them). `qgs`/`qgd`/`cqgs`/`cqgd`/`p`
are not supported (explicit error). `.dc @j1[area|m|temp|dtemp|ic-vds|ic-vgs]`
sweeps re-derive `JFETtemp`.

## Not ported

`.noise` (`jfetnoi.c`), `.disto` (`jfetdset.c`/`jfetdist.c`) and `.sens` of a
JFET fail with `NotYetPorted`.

## Evidence

- `tests/jfet.rs`: grammar and gaps, level selection and validation, analytic
  square-law regions of both polarities, inverse-mode symmetry and the Sydney
  `B` term, internal-node order, finite-difference DC Jacobians (Newton load
  and small-signal `A`) over a bias grid of both polarities with RD/RS,
  area, `m` and temperature, AC gate capacitance, observations, explicit
  noise/distortion refusal, a transient follower, writer round trip; unit
  tests in `devices::jfet` for the channel partials, drain/source
  antisymmetry and the charge/capacitance continuity.
- C goldens (`cargo xtask golden verify`): `m10_jfet_dc`, `m10_jfet_ac`,
  `m10_jfet_tran`, `m10_jfet_temp`; see
  [VERIFICATION.md](VERIFICATION.md#m10-jfet-level-1-82).
- Opt-in `tests/c_jfet_reference.rs` (live C): saved asks of both
  polarities, `.dc @j1[area] @j1[temp]`, and `.pz` roots.

## Level 2 (Parker-Skellern)

`.model name njf|pjf(level=2 ...)` builds `devices::jfet2` from `jfet2/`:
`jfet2parm.h` (setters and defaults), `jfet2set.c`, `jfet2temp.c`,
`psmodel.c` (`PSids`, `qgg`/`PScharge`, `PSacload`, `PSinstanceinit`),
`jfet2load.c`, `jfet2acld.c`, `jfet2trun.c`, `jfet2ask.c`, `jfet2ic.c`. The
instance grammar, `area`/`m`/`temp`/`dtemp`/`off`/`ic`, the RD/RS internal
nodes, `MODEINITJCT`/`uic`/`off` starts and `DEVpnjlim`/`DEVfetlim` limiting
(against the model's VTO; level 2 has no VTO temperature law) are shared
with level 1. No full Parker-Skellern parity is claimed beyond the evidence
below.

**Parameters** (`jfet2parm.h`, `jfet2set.c` defaults): `ACGAM` 0, `AF` 1,
`BETA` 1e-4, `CDS`/`CGD`/`CGS` 0, `DELTA` 0, `HFETA`/`HFE1`/`HFE2`/`HFG1`/`HFG2`
0, `HFGAM` (default: the model's `LFGAM`), `LFGAM`/`LFG1`/`LFG2` 0, `MVST` 0,
`MXI` 0, `FC` 0.5, `IBD` 0, `IS` 1e-14, `KF` 0, `LAMBDA` 0, `N` 1, `P` 2,
`VBI`/`PB` 1 (one parameter, last spelling wins), `Q` 2, `RD`/`RS` 0, `TAUD`/
`TAUG` 0, `VBD` 1, `VER` 0 (read nowhere in C), `VST` 0, `VT0`/`VTO` -2 (last
spelling wins), `XC` 0, `XI` 1000, `Z` 1, `TNOM`. Level-1-only setters (`B`,
`TCV`, `XTI`, ...) are rejected. Fail-closed range checks where `psmodel.c`
would divide by zero or take a root of a negative number: `VBD`, `XI`, `P`,
`Q`, `PB`, `IS`, `N` positive; `Z`, `TAUD`, `TAUG`, `VST`, `DELTA`, `IBD`,
`BETA`, `LAMBDA`, `RD`, `RS`, `CGS`, `CGD`, `CDS` nonnegative; `FC > 0.95` is
rejected; nonfinite derived values (e.g. `D3` with `VBI = VTO` and `P != Q`)
and nonfinite evaluations are errors.

**Equations.**

- *Temperature* (`jfet2temp.c`): `IS(T) = IS exp((T/TNOM - 1) 1.11 / Vt)`
  (fixed 1.11 eV, no `N`, no `XTI`), `VBI(T)` and the CGS/CGD factors by
  level 1's band-gap laws, `vcrit` from the unscaled `IS(T)`; then
  `PSinstanceinit`: `xiwoo = XI (VBI(T) - VTO)`, `za = sqrt(1 + Z) / 2`,
  `alpha = (xiwoo / (XI + 1))^2 / 4`, `d3 = P / Q / (VBI(T) - VTO)^(P - Q)`.
- *Channel* (`PSids`): exponential subthreshold `vgt = VST(1 + MVST vds)
  ln(1 + exp(vgst / vst))` (numerically linear above `40 vst`, cut off below
  `-10 vst`), dual power law (`P`, `Q`, `d3`), early saturation (`XI`, `MXI`,
  `Z`), `LAMBDA`, `BETA * area`, and thermal reduction `ids / (1 + DELTA/area
  * pave)`. `gm`/`gds` are C's partials (exact; finite-difference checked).
  Inverse mode exchanges the junctions as `jfet2load.c` does.
- *Rate-dependent threshold and self-heating* (`psmodel.c` state): the
  threshold shift `-(LFGAM - LFG1 vgstrap + LFG2 vtrap) vtrap + eta (vgstrap -
  vgs) + gam (vtrap - vgd)` uses the filtered gate voltages, and `pave` the
  filtered power, each `x = h x(accepted) + (1 - h) x(now)` with `h = (tau /
  (tau + dt/4))^4` (TAUG, TAUD) in transient and `h = 0` otherwise. The filters
  read only the last accepted point, so rejected trials leave no trace. As in
  C the slots follow `PSids`' argument order, so in inverse mode `vtrap` holds
  the gate-source voltage.
- *Gate diodes*: `IS(T) * area` with `N Vt`, dropped below `-10 N Vt`,
  linearized above `40 N Vt`, plus reverse "breakdown" `IBD * area *
  (exp(-v/VBD) - 1)`, plus `gmin`.
- *Charge*: Statz `qgg` with `ACGAM`, `XC`, `alpha`, `VMAX = FC * VBI(T)`, and
  `CDS * area * vds`. In transient the gate charges are incremental (half
  sums of `qgg` at the present and accepted junction voltages) and both drive
  `jfet2trun.c` truncation (CDS does not). A plain operating point stores
  zero gate charges, a DC sweep or the `uic` initial load the total charge in
  both slots, as C does, so truncation control sees C's charge magnitudes.
- *AC* (`jfet2acld.c`/`PSacload`): DC conductances, `PSacload`'s
  frequency-dependent `gm`/`gds` (real parts in `A`, imaginary parts in `E`
  divided by `omega`; the device asks for per-frequency assembly when TAUG or
  TAUD is set), `j omega` times the Statz capacitances (two-terminal, as in
  C) and CDS. With `TAUG = TAUD = 0` it reduces to the DC conductances.

**Divergences.** Those of level 1 (no predictor/bypass, held `off` rule,
FC rejection), plus: the Newton matrix is the exact Jacobian (C omits the CDS
companion conductance and the incremental-charge cross derivatives from its
matrix; the residual and its root are C's); `.pz` is refused (C has no
`DEVpzLoad` for level 2 and silently omits the device).

**Observations** (`jfet2ask.c`): as level 1 (`id`/`ig`/`is` normalized,
`area * m`, ...), plus `vtrap` and `vpave` (unscaled, as C). In transient only
`vgs`/`vgd` and the terminal currents are available; `gm`, `gds`, `ggs`,
`ggd`, `igd`, `vtrap`, `vpave` read C's filter or charge state there and are
refused (`NotYetPorted`).

**Not ported.** `.noise` (`jfet2noi.c`), `.disto` (C has none) and `.sens`
fail explicitly.

**Evidence.** `tests/jfet2.rs`: level selection and validation, a square-law
reduction (`P = Q = 2`, `VST = Z = 0`, huge `XI`) of both polarities, inverse
mode, the thermal-reduction law, DC and transient finite-difference Jacobians
(filters, incremental charges, CDS, both modes, breakdown, subthreshold),
TAUG dispersion (`|gm(inf)| / |gm(0)| = (1 - HFGAM) / (1 - LFGAM)`),
self-heating relaxation to the DC point, asks, explicit refusals, writer round
trip; unit tests in `devices::jfet2` for `PSids`/`qgg` partials and
`PSacload`. C goldens `m10_jfet2_{dc,ac,tran,temp}` and the opt-in
`tests/c_jfet2_reference.rs`; see
[VERIFICATION.md](VERIFICATION.md#m10-jfet-level-2-82).
