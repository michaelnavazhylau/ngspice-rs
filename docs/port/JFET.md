# JFET level 1 (#82, M10 slice 1)

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
backend: 0 or 1 is this device, 2 (Parker-Skellern, `jfet2/`) fails with
`NotYetPorted` naming `jfet2/`, any other level fails with `NotYetPorted`
naming `inpdomod.c` (C itself rejects it). The tail flags `njf`/`pjf` set the
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
JFET fail with `NotYetPorted`; JFET level 2 (`jfet2/`, M10 slice 6).

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
