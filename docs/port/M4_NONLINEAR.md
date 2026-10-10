# M4 — bounded nonlinear devices and convergence

Implemented in the Rust-only `work/m4-nonlinear` worktree, based on `9f0f6d1`
(the integrated M1 options and M3 trial/accepted-state/companion contracts).
This is a demonstrated **subset**, not full SPICE parity. There is no due date.
Publication targets the Rust-only repository; remote issue closure is separate
from this bounded local gate and its review.

## Workspace and ownership

Local workspace: `worktrees/ngspice-rs-m4/`. Its ignored `target` symlink shares
`../ngspice-rs-m3/target`; `c-reference` points to `../../ngspice_test`. Cargo's
existing global dependency cache is reused. These links are local conveniences,
not repository dependencies. Source, decks and goldens are independent worktree
files: symlinking editable files between branches would violate isolation.

- `devices::models` / `schema`: existing first-declaration family/level
  resolution, ordered last-set scalar projection, units, provenance and domains.
- `devices::nonlinear`: diode factory, junction equations and nonlinear
  charge integration. `bjt`: the Gummel-Poon BJT (#87). `mos1`: the MOS
  level-1 factory/equations (completed by #88, see [MOS1](#mos1)).
- `Device::assemble_small_signal` / `Circuit::small_signal_system`: conductance
  and charge Jacobians at an explicit bias. This is **not** immutable BDF assembly.
- `devices::limiting`: ngspice's junction/FET voltage limiting
  (`DEVpnjlim`, `DEVfetlim`, `DEVlimvds`) and the `MODEINITJCT` start, used by
  the diode, BJT and MOS1 loads (#106).
- `analysis::newton`: disposable load/solve/reload, physical iterate and
  equation-residual convergence, row equilibration, device limiting by default
  and the bounded global voltage damping as a fallback.
- `analysis::bias`: direct DC solve and ngspice's `CKTop` continuation
  (`dynamic_gmin`, `new_gmin`, `spice3_gmin`, `gillespie_src`, `spice3_src`),
  or the port's fixed gmin/source ladders.
- `analysis::sweep`: typed V/I/R/TEMP axes and bounded nested DC.
- Companion transient consumes all `Device::truncation_slots` charge/derivative
  pairs. State remains owned by the existing M3 history, not the model object.

The raw-card registry has no model-card context and deliberately still rejects
D/Q/M there. The working model-aware factory is `Circuit::from_netlist` /
`RunConfig::circuit` / `Circuit::add_instance`; raw-card coverage is not a promise
of a context-free nonlinear factory.

## Supported physics and explicit boundaries

Every unknown setter, including a parsed but unimplemented flag/IC vector, fails
before node interning. Scalar schema validation checks **all occurrences**, not
just the last one; original ASTs remain unchanged. Defaults follow the named C
setup routines; the code's schema tables are the exact allowlists.

### Diode

M4 delivered the bounded core; M7 (#86) extended it toward `dio` parity. The
schema tables in `src/devices/nonlinear.rs` are the exact allowlists;
C's `dio.c` aliases (`js`, `isw`, `tref`, `trs1`, `cj0`/`cj`, `pb`, `mj`, `cjp`,
`php`, `ik`, `nz`, `vb`/`vrb`/`var`, `ib`, `tbv1`, `ctc`, `tvj`) fold onto their
canonical setter in order, so last-set precedence spans an alias and its name.

Model (defaults from `diosetup.c`): `IS` (1e-14 A, floored at CKTepsmin 1e-28),
`N` (1), `RS` (0 ohm), `TRS`/`TRS2` (0), `TNOM` (context), `TT` (0 s),
`TTT1`/`TTT2` (0), `CJO` (0 F), `VJ` (1 V), `M` (0.5), `TM1`/`TM2` (0), `FC` (0.5),
`JSW` (given flag), `NS` (1; given flag), `CJSW` (0 F), `VJSW` (1 V), `MJSW` (0.33),
`FCS` (0.5), `BV` (given flag), `IBV` (1 mA), `NBV` (= N), `TCV` (0), `TLEV` (0..2),
`TLEVC` (0..1), `EG` (1.11 eV, 1.16 eV under TLEV 2), `GAP1` (7.02e-4), `GAP2`
(1108), `XTI` (3), `CTA`/`CTP`/`TPB`/`TPHP` (0), `ISR` (given flag), `NR` (2),
`IKF`/`IKR`/`IKP` (given flags; below 1e-28 disabled like C), `JTUN`/`JTUNSW`
(given flags), `NTUN` (30), `XTITUN` (3), `KEG` (1), `AREA` (1), `PJ` (0).
Instance: `AREA` and `PJ`/`PERIM` (default to the model's), `M` (1), `TEMP` or
`DTEMP` (giving both is an error; C silently ignores DTEMP). Grading and FC
coefficients require `0 <= value < 1`.

- Exponential forward law, C's cubic reverse continuation below `-3 N Vt` and,
  with BV, the reverse-breakdown exponential `-IS exp(-(xbv + V)/(NBV Vt))`
  below `-xbv` (`dioload.c`). `xbv` is matched to IBV*M (level 1: not AREA)
  at the TCV-adjusted BV exactly like `diotemp.c` (TLEV 0: `BV - TCV dT`,
  otherwise `BV (1 - TCV dT)`; unmatched BV when IBV < ISAT BV/Vt).
- Sidewall: JSW*PJ*M current sharing the bottom characteristic, or its own
  NS characteristic (including breakdown) when NS is given; CJSW*PJ*M depletion
  charge with VJSW/MJSW/FCS; IKP knee on the sidewall current.
- Recombination ISR/NR with C's generation factor (constant at `-3 N Vt` in
  reverse), tunnelling JTUN/JTUNSW/NTUN/XTITUN/KEG, IKF/IKR high-injection knees.
- Temperature (`diotemp.c::DIOtempUpdate`): instance temperature TEMP, else the
  circuit temperature plus DTEMP; every saturation current with EG/XTI (TLEV
  0/1) or the GAP1/GAP2 band gap (TLEV 2); depletion capacitance/potential by
  C's band-gap law (TLEVC 0) or the linear CTA/CTP/TPB/TPHP law about 27 C
  (TLEVC 1); TM1/TM2 grading, TTT1/TTT2 transit time (clamped like C), TRS/TRS2
  series resistance. Quantities are derived per evaluation, never cumulatively,
  so `.dc temp` and `.options temp` sweeps are exact.
- A fixed 1e-12 S junction gmin (`.option gmin`), separate from artificial nodal
  continuation; diffusion charge `TT * I` includes the gmin current, as in C.
- RS creates an internal anode, with conductance `AREA*M/RS(T)`.
- Depletion charge with C's continuous quadratic continuation above `FC*VJ`.
- Newton uses the exact derivative of every current. AC (`dioacld.c`) uses C's
  stored small-signal conductance, which differs only for ISR: `dioload.c`
  omits the generation factor's `1/VJ` and reapplies the factor to the scaled
  recombination current. The DC root is unaffected; the AC golden
  `m7_diode_temp_ac` fails if the exact derivative is used instead.

Deliberate divergences: breakdown matching stops at C's default RELTOL (1e-3)
because device temperature setup has no access to the run's RELTOL; a deck that
changes `reltol` can see C's rapidly contracting match stop one iteration
earlier or later (a knee shift within the matching tolerance).

Not yet ported (`SpiceError::NotYetPorted`, naming the C file): soft reverse
recovery (`VP`, `QPSCALE`), separate sidewall resistance (`RSW`), self-heating
(`RTH0`, `CTH0`, instance `THERMAL`), level-3 geometry (`LM`/`LP`/`WM`/`WP`,
`XOM`/`XOI`/`XM`/`XP`/`XW`, instance `W`/`L`), SOA limits
(`FV_MAX`, `BV_MAX`, `ID_MAX`, `TE_MAX`, `PD_MAX`), C's common-characteristic sidewall current in
breakdown (JSW*PJ > 0 with BV and without NS: `dioload.c` evaluates it with an
unassigned `vdsw`), and TM1/TM2 with sidewall capacitance (C mixes adjusted and
nominal sidewall grading). Unknown setters remain unsupported errors. The
junction voltage is limited as in `dioload.c` (`MODEINITJCT` start at `tVcrit`,
`DEVpnjlim`, reflected about BV in breakdown; #106).

Instance `OFF` and `IC` (#99, see [Initial conditions](#nonlinear-initial-conditions-99)):
an `OFF` diode is held at 0 V in the `MODEINITJCT` and `MODEINITFIX` loads; the
`uic` initial load starts the junction at the external terminal voltage of the
`.ic`/`.nodeset` node vector. The instance `IC` is accepted and has **no
effect**, exactly as in C: `dioparam.c` stores it without setting
`DIOinitCondGiven`, so `diogetic.c` always overwrites it with the node
difference (checked against the reference binary: `ic=0.4`, `ic=0.9` and no
`ic` give bit-identical C rawfiles).

References: `dio/dioload.c`, `diosetup.c`, `diotemp.c`, `dioacld.c`,
`diompar.c`, `dioparam.c`, `dio.c`. Unit tests in `nonlinear.rs` check
finite-difference current and charge Jacobians per slice (breakdown, sidewall,
recombination, tunnelling, knees, TLEV/TLEVC laws at several temperatures);
`tests/diode_physics.rs` checks the production paths against
closed forms (breakdown law of a Zener operating point, a `.dc temp` forward
voltage, sidewall/bottom equivalence in AC, transient depletion-charge
conservation at 77 C). C goldens: `m7_zener_dc`, `m7_zener_tran`,
`m7_diode_physics_dc`, `m7_diode_temp_dc`, `m7_diode_temp_ac` (see
[VERIFICATION.md](VERIFICATION.md#m7-diode-physics-86)).

### BJT

Gummel-Poon level 1 (#87, `devices::bjt`), NPN/PNP, three or explicitly
four terminals. References: `bjt/bjt.c` and `bjtmpar.c` (setters and aliases),
`bjtsetup.c` (defaults, internal nodes), `bjttemp.c`, `bjtload.c`, `bjtacld.c`,
`bjttrunc.c`.

Model: `IS` (1e-16 A), `IBE`/`IBC` (used when both are given), `BF` (100),
`NF` (1), `VAF`/`VA`, `IKF`/`IK`, `NKF`/`NK` (sqrt law unless given; clamped to
1), `ISE`/`C2` and `ISC`/`C4` (a value above 1e-4 is a multiple of `IS`, as in
C), `NE` (1.5), `BR` (1), `NR` (1), `VAR`/`VB`, `IKR`, `NC` (2), `RB`, `IRB`,
`RBM` (default `RB`), `RE`, `RC`, `CJE`, `VJE`/`PE` (0.75), `MJE`/`ME` (0.33),
`TF`, `XTF`, `VTF`, `ITF`, `PTF` (only without `TF`, see below), `CJC`,
`VJC`/`PC` (0.75), `MJC`/`MC` (0.33), `XCJC` (1, clamped to [0, 1]), `TR`,
`CJS`/`CSUB`/`CCS`, `VJS`/`PS` (0.75), `MJS`/`MS` (0), `ISS`, `NS` (1), `SUBS`
(vertical NPN / lateral PNP by default), `XTB`, `EG` (1.11), `XTI` (3), `FC`
(0.5, limited to 0.9999), `TNOM`/`TREF`, `TLEV` (0, 1, 3), `TLEVC` (0, 1) and
every first/second-order temperature coefficient of `bjt.c` (`TBF1` ...
`TISS2`, `CTC`/`CTE`/`CTS`, `TVJC`/`TVJE`/`TVJS`, `TRB`/`TRC`/`TRE` aliases).
Instance: `AREA`, `AREAB`, `AREAC` (default `AREA`), `M`, `TEMP`, `DTEMP`.
Raw `MJE`/`MJC`/`MJS`/`FC` of 1 or more are rejected; `TLEV`/`TLEVC` outside the
C selectors are errors rather than C's warning-and-reset.

- Base charge `qb` from `q1` (Early) and `q2` (high injection), C's transport
  current `(cbe - cbc)/qb`, ideal and leakage base currents, junction gmin on
  both base currents and the substrate junction; `m` multiplies every stamp.
- `RC`/`RB`/`RE` create the internal nodes `<q>#collCX`, `<q>#base`,
  `<q>#emitter` in C's order; `gx` follows `RBM + (RB - RBM)/qb` or the `IRB`
  current-crowding law. `.save`/rawfiles omit these internal nodes as C does.
- Charges: `qbe` with the bias-dependent transit time (`XTF`/`VTF`/`ITF`,
  including the `d qbe / d vbc` cross capacitance), `qbc` with `TR`, the
  external-base `qbx` (`XCJC < 1`) and the substrate `qsub` (C's linear
  forward-bias form). Four charge/current state pairs; `qbe`, `qbc`, `qsub` and,
  when `XCJC < 1`, `qbx` take part in truncation control (`bjttrunc.c`).
- `bjttemp.c` temperature and area scaling, including `pbfact` built-in
  potentials, `TLEVC=0` capacitance laws and the `TLEV` 0/1/3 saturation-current
  and beta laws. Every load re-evaluates at its context temperature, so `.temp`,
  `.options temp`, `TEMP`/`DTEMP` and `.dc temp` sweeps all apply.
- Newton loads stamp the exact Jacobian, including `d gx / dV`; C stamps only
  `gx` there, so both converge to the same root. The AC assembly reproduces
  `bjtacld.c` (conductance `gx` only, charge Jacobians including the cross term).

Not ported (`SpiceError::NotYetPorted` naming the C file): excess phase (`PTF`
with `TF != 0`: Weil's approximation in `bjtload.c` and the AC phase rotation in
`bjtacld.c`), Kull's quasi-saturation model (`RCO`, `VO`, `GAMMA`, `QCO`,
`QUASIMOD`, `VG`, `CN`, `D`), safe-operating-area
limits (`*_MAX`, `RTH0`, `bjtsoachk.c`). `OFF` and `IC`/`ICVBE`/`ICVCE` follow
`bjtload.c`/`bjtgetic.c` (#99): the `uic` initial load starts at
`vbe = type * ICVBE`, `vbc = vbx = vbe - type * ICVCE`, `vsub = 0`, unset
components from the external terminals of the node vector; `OFF` holds all
junctions at zero in `MODEINITJCT`/`MODEINITFIX`. VBIC (level 4) is a non-goal. Newton limiting follows `bjtload.c`
(#106): `MODEINITJCT` starts at `vbe = tVcrit`, and `DEVpnjlim` limits `vbe`,
`vbc` and `vsub` (`VCRIT_DISABLED` without ISS); the quasi-saturation limits
belong to the rejected quasi-saturation model.

Gate: C goldens `m7_bjt_gummel` (VBC = 0 Gummel plot), `m7_bjt_output` (nested
VCE/IB output characteristics), `m7_bjt_temp` (-40..125 C sweep of NPN, lateral
PNP with substrate and TLEV=3/TLEVC=1 devices), `m7_bjt_amp_ac` and
`m7_bjt_amp_tran` (CE amplifier at 50 C with every charge). They set
`.option reltol=1e-8`: at C's default reltol, with bypass, these sweeps stop
about 4e-4 from the converged root, outside the unchanged 1 ppm `NONLINEAR`
bound; at 1e-8 the worst DC/AC errors are below 0.1 of the bound and the
transient's 0.04 of `compare::TRAN`. `devices` unit tests check every
slice's analytic derivatives by finite differences at nominal and 85 C and the
`bjttemp.c` laws; `tests/bjt_gummel_poon.rs` checks independent
Gummel-Poon/KCL equations, exact Newton and companion Jacobians of the whole
circuit, small-signal operators, disposable charge state, transient terminal-
current conservation and the explicit errors; the opt-in `c_bjt_reference`
compares substrate forward bias, reverse/saturation, `TLEV`/`TLEVC` variants
and AC charges with live C at the same bound.

### MOS1

Level 1 NMOS/PMOS in `devices::mos1` (#88). C references, read as
behaviour only: `mos1/mos1set.c`, `mos1temp.c`, `mos1load.c`, `mos1acld.c`,
`mos1trun.c` and `devices/devsup.c` (`DEVqmeyer`).

Model setters (C defaults; *derived* means C's `...Given` logic applies):
`VTO`/`VT0` (0 V or derived), `KP` (2e-5 A/V² or derived), `GAMMA` (0 or
derived), `PHI` (0.6 V or derived), `LAMBDA` (0), `RD`/`RS`/`RSH` (0 ohm),
`CBD`/`CBS`, `IS` (1e-14 A), `JS` (0 A/m²), `PB` (0.8 V), `CGSO`/`CGDO`/`CGBO`
(0 F/m), `CJ` (F/m²), `MJ` (0.5), `CJSW` (F/m), `MJSW` (0.5), `FC` (0.5), `TOX`,
`LD` (0 m), `U0`/`UO` (600 cm²/Vs when KP is derived), `NSUB` (cm⁻³), `TPG` (1),
`NSS` (0 cm⁻²) and `TNOM`. Aliases apply in card order. Instance setters: `L`/`W`
(100 um), `M` (1), `AD`/`AS` (0 m²), `PD`/`PS` (0 m), `NRD`/`NRS` (1), `TEMP` and
`DTEMP` (0). `MJ`, `MJSW` and `FC` must be below 1.

- **Channel** (`mos1load.c`): cutoff/triode/saturation square law with
  `LAMBDA`, drain/source reversal, and the body effect for reverse *and forward*
  body bias, including C's clamp of `sarg` at zero beyond `2 PHI`. Effective
  length is `L - 2 LD`. The Jacobian (gm, gds, gmb) is the exact derivative; in
  forward bias C stamps `gm * GAMMA / (2 sarg)` instead, which changes only the
  Newton path.
- **Series resistance** (`mos1set.c`, `mos1temp.c`): `RD`/`RS` win over
  `RSH * NRD`/`RSH * NRS`, scaled by `M`; a nonzero conductance creates an
  internal node `<name>#drain` / `<name>#source` (`NodeKind::Internal`) that
  the channel, junctions and gate charges see. Combinations C leaves infinite
  (RSH with zero squares) or floating (explicit `RD=0` beside RSH) are errors.
- **Junctions**: constant reverse saturation current below `-3 Vt`; `IS * M` on
  both sides unless `JS`, `AD` and `AS` are all nonzero (then `JS * AD * M`,
  `JS * AS * M`). Depletion charge sums a bottom part (`CBD`/`CBS` if given, else
  `CJ * AD`/`CJ * AS`) and a sidewall part (`CJSW * PD`/`CJSW * PS`), with
  `MJ`/`MJSW`, and the linear `f2/f3/f4` continuation above `FC * PB`.
- **Meyer gate charge** (`mos1load.c`, `DEVqmeyer`): `Cox = 3.9 eps0 / TOX * Leff
  * W * M` (zero for absent/zero TOX). Each load stores the half capacitances and
  the gate voltages; at an operating point `q = v (2 half + overlap)`, in
  transient `q = q1 + (v - v1)(half + half1 + overlap)` with Jacobian `a0 (half +
  half1 + overlap)`, exactly C's state-averaging formulation. AC uses `2 half +
  overlap` at the bias (`mos1acld.c`).
- **Process extraction** (`mos1temp.c`, only with nonzero TOX): KP from U0;
  with NSUB (> 1.45e10 cm⁻³, else an error), PHI, GAMMA and VTO from NSUB, TPG,
  NSS and the band gap at TNOM, each only when not given. Without TOX these
  setters have no effect, as in C.
- **Temperature** (`mos1temp.c`): device temperature `TEMP`, else circuit
  temperature plus `DTEMP`; `KP` scales with `(T/TNOM)^-1.5`; `PHI`, `VBI`, `PB`
  follow the band-gap laws; `IS`/`JS` scale by `exp(-Eg/Vt + Eg1/Vtnom)`;
  `CBD`/`CBS`/`CJ`/`CJSW` use C's two-step capacitance factor. Every load
  re-evaluates these from the analysis temperature, so `.options temp`, TEMP
  sweeps and instance `TEMP`/`DTEMP` all apply.
- **State and truncation**: 20 state slots (bulk-drain/bulk-source and three
  gate charge/derivative pairs, three gate voltages, three half capacitances,
  and the limited `vbs`/`vgs`/`vds`/`von` of the load for #106 limiting).
  Only the three gate charges control the timestep, as in `mos1trun.c`; the
  bulk junction charges are integrated but do not enter LTE control.

Newton limiting follows `mos1load.c` (#106): `MODEINITJCT` starts at
`vbs = -1`, `vgs = type * tVto`, `vds = 0`; later loads apply `DEVfetlim` to
`vgs` (or `vgd` in reverse mode) against the previous `von`, `DEVlimvds` to
`vds` and `DEVpnjlim` to the forward bulk junction, and the device evaluates
and linearizes at the limited voltages. Deliberate divergences: C's
`MODEINITPRED`/`MODEINITTRAN` extrapolation (a predicted load limits against
the last accepted voltages instead), the zero gate-charge stamp of the first
`MODEINITTRAN` iteration, `.options oldlimit` and bypass are not ported, which
changes the Newton path but not the converged point. C warns and continues for `L - 2 LD <= 0`;
here it is an error, as are nonfinite/nonpositive PHI, PB, KP or IS after
temperature scaling.

Instance `OFF`, `IC`, `ICVDS`, `ICVGS`, `ICVBS` follow `mos1load.c`/`mos1ic.c`
(#99): `MODEINITJCT` starts at the `IC` vector (`type * ICVDS/ICVGS/ICVBS`)
whenever a component is nonzero, **also without `uic`** (C has no `MODEUIC`
test there), at the default start otherwise; under `uic` unset components come
from the external terminals of the node vector and an all-zero vector stays
zero; `OFF` starts and holds the device at zero (it wins over the `IC` vector).
The noise parameters `KF`, `AF`, `NLEV`, `GDSNOI` (`mos1noi.c`) and the diode
and BJT `KF`/`AF` drive `.noise` (#100, [NOISE.md](NOISE.md)). Other MOS levels, BSIM/CIDER/XSPICE remain
outside scope.

Evidence: `tests/m7_mos1.rs` (operating-point and transient
Meyer charge recurrences, AC capacitances, process-extraction and temperature
laws restated from `mos1temp.c`, internal-node series resistance KCL, exact
transient companion Jacobian where the Meyer halves are constant, DC Jacobian
finite differences with series resistance, forward/reverse body bias, PMOS
inverse mode, geometry and temperature, explicit unported inputs) and four C
goldens verified by `cargo xtask golden verify`:

| Fixture | Exercises | Worst error |
| --- | --- | --- |
| `m7_mos1_inverter_tran` | CMOS inverter, TOX, LD, RSH/NRD/NRS and RD/RS, CJ/CJSW/JS geometry | 0.438 of `TRAN` bound |
| `m7_mos1_ring_tran` | 3-stage CMOS ring oscillator (50 fF loads), current kick, TOX | 0.523 of `TRAN` bound |
| `m7_mos1_meyer_ac` | Meyer + junction AC at 75 C, `M=2`, LD, RD/RS | within `NONLINEAR` |
| `m7_mos1_process_dc` | NSUB/TPG/NSS/UO extraction, forward body bias, TEMP/DTEMP/TNOM, PMOS RSH | within `NONLINEAR` |

See [VERIFICATION.md](VERIFICATION.md#mos1-completion-88) for why the transient
decks bound the maximum step, why the ring oscillator is three stages, and why
the DC deck tightens RELTOL.

### JFET

Level 1 NJF/PJF (#82, M10) lives in `devices::jfet`; see [JFET.md](JFET.md)
for its parameters, equations, limiting, the `off` divergence from C and its
C goldens.

## Solvers, sweeps and state

`NewtonOptions` defaults: 100 iterations (C's `itl1`), reltol=1e-8,
vntol=1e-10 V, abstol=1e-12 A and device limiting (`StepLimiting::Device`,
#106); `limiting=global` selects the earlier policy, a maximum global nodal
voltage change of 0.2 V per iteration with exact device loads.
Unknown/duplicate/nonfinite named convergence arguments fail.

DC/AC request options include `rtol`, `vntol`, `abstol`, `maxiter`/`itl1`
(1..=10000), `gminsteps`, `srcsteps`, and `gminfactor`. Local #34 follow-ups add
`DcSettings`/`ContinuationPolicy` schedules/budgets and `solve_dc_with` success/failure reports;
OP/DC/AC bias use the same configured settings, with request > deck > defaults.
The companion transient initial bias reads the same DC options. See
[DC_CONTINUATION.md](DC_CONTINUATION.md) for exact semantics and the remaining
differences from C (no `OPtran` fallback, predictor, bypass or `gshunt`).
`.nodeset` seeds the nonlinear bias and is forced as an exact node-row
constraint in its `MODEINITJCT`/`MODEINITFIX` loads, then released (#99,
`cktload.c`); `.ic` is ignored in DC/AC after name validation, as in C. Junction `gmin` and `itl1`/`itl2`/`itl4` are
configurable (#110).

By default, on direct Numerical failure, DC runs ngspice's `dynamic_gmin`, then
`new_gmin`, then `gillespie_src` (`gminsteps`/`srcsteps` select `spice3_gmin`/
`spice3_src` or disable a family; `continuation=ladder` restores the earlier
fixed nodal-gmin and source ladders). Typed policies can disable or replace
these schedules and bound total work. Every success ends with **full source
values, zero artificial nodal gmin and the configured junction gmin**. A regularized
nonunique physical circuit still fails. Temporary stages never call accept hooks.
Linear circuits retain exact solving and source-only sweeps reuse LU factors.

Typed targets: independent V/I sources, circuit temperature and scalar resistance
(including supported model-backed resistors). One optional outer axis, inner axis
varying fastest; <=100000 Cartesian points, inclusive reachable endpoints, finite
signed steps, explicit no-progress/duplicate-target errors. Nested plots append an
explicit `sweep(outer-name)` column. Immutable point overrides preserve originals
on success/failure; R/TEMP axes rebuild numerical operators while source-only linear
axes retain LU reuse. See [DC_SWEEPS.md](DC_SWEEPS.md) for supplied/effective semantics,
analytic tests and eight opt-in live C comparisons. Arbitrary model-setter sweeps
and >2 axes remain unsupported. Local #34/#35 follow-ups are not yet published.

AC solves G(bias)+j*w*dQ/dx(bias) after a valid nonlinear DC point.
Companion transient uses the same Newton kernel, M3 breakpoint/order/work policy,
actual nonlinear Q history and **a0*dQ/dV**, not a0*Q/V. Loads and residual reloads
write only disposable trials; accept hooks run before atomic history commit.
Nonlinear `.ic`/`uic`/instance IC run on the companion driver (#99, below); the
explicit diffsol nonlinear backend is rejected. Trap/Gear orders above 2 and
advanced DAE support remain unclaimed.

## Nonlinear initial conditions (#99)

C references: `cktic.c`, `cktload.c` (`.nodeset`/`.ic` row stamping),
`niiter.c` (`MODEUIC` shortcut, `MODEINITJCT`/`MODEINITFIX` phases),
`niconv.c` with `DIOconvTest`/`BJTconvTest`/`MOS1convTest` (`NEWCONV`),
`dctran.c`, `dioload.c`/`diogetic.c`, `bjtload.c`/`bjtgetic.c`,
`mos1load.c`/`mos1ic.c`. Code: `devices::limiting`
(`Linearization::InitialConditions`, `Limiter::holds_off`,
`Limiter::test_held`), `analysis::bias::NodeForcing`,
`analysis::initial`, `companion.rs`.

| Input | ngspice | This port |
| --- | --- | --- |
| `.ic`, no `uic`, nonlinear | row `v = ic * srcFact` (or a `1e10` conductance on branch rows) in every load of the `MODETRANOP` `CKTop`, released for the transient | exact row replacement in every Newton load of every continuation stage, scaled by the source factor; source-fixed nodes resolved structurally (agreeing entries dropped, contradictions errors) as for linear circuits |
| `.nodeset`, nonlinear | row forced in `MODEINITJCT`/`MODEINITFIX` loads of every DC operating point (`.op`, `.dc` first points and restarts, `.ac` bias, transient bias), then released | same, after dropping hints on nodes ideal sources/E/H outputs/`.ic` rows already fix (C's `1e10` compromise there is not reproduced) |
| `uic` | one `MODETRANOP + MODEUIC + MODEINITJCT` load, no solve: devices at their IC voltages, charges from them | one device-flagged initial load at the `uic` node vector; junction charges and limited voltages from it fill the accepted history |
| D/Q/M `OFF` | junctions at 0 in `MODEINITJCT`/`MODEINITFIX`; the `MODEINITFIX` load skips its own noncon check but `NIconvTest` runs the device convergence test against the held state | same: the held `MODEINITFIX` load is nonconvergent exactly when C's test (`reltol`, `abstol` of the solve) fails, so an `OFF` device whose iterate voltages sit volts from zero keeps the direct iteration in `MODEINITFIX` until `itl1`, and continuation (dynamic gmin) decides the state, as in C |
| MOS1 `ic=` without `uic` | `MODEINITJCT` start voltages | same |
| D/Q `ic=` without `uic` | ignored | ignored |
| diode `ic=` under `uic` | overwritten by the node difference (no effect) | same |

`OFF` and MOS1 start voltages need the device-limited Newton loads: under the
port's legacy `limiting=global` they are an explicit `Unsupported` error.
The BDF backend keeps rejecting `.ic`/`uic`. The `uic` impulse check covers
capacitors and inductors only; junction charges are not part of it (C checks
nothing). A `uic` node vector that forward-biases a junction by volts (for
example a MOS1 drain `.ic` above a supply node that has no `.ic`, so C's node
vector holds the supply at 0 V) can make the first trial's Jacobian too
ill-conditioned for the port's rank-checked sparse LU; the run then fails with
the numerical error where C's pivoting factors on. Evidence: the six `m7_ic_*`
goldens ([VERIFICATION.md](VERIFICATION.md#m7-nonlinear-initial-conditions-99))
and `tests/nonlinear_initial.rs`.

## Demonstrated local gate (#41)

`cargo xtask golden verify` now verifies 25 fixtures, with only subcircuit
flattening excluded. Nine exercise nonlinear devices:

| Family | DC | AC | Charge transient |
| --- | --- | --- | --- |
| Diode | existing `diode_dc` | `m4_diode_ac` (RS/CJO/TT) | `m4_diode_tran` |
| BJT | existing `bjt_ce` | `m4_bjt_ac` (CJE/CJC/TF) | `m4_bjt_tran` |
| MOS1 | existing `mos_inverter` | `m4_mos1_ac` (CBD/CBS/overlap) | `m4_mos1_tran` |

Six new pure decks were captured **individually** with the existing out-of-process
ngspice-47+ binary; no previous golden was recaptured. Twelve new parser snapshots
were added (98 existing snapshots unchanged). Ordinary tests need no C toolchain.

DC/AC: 1e-6 relative + 1e-12 absolute per real/imaginary component, allowing
independent nonlinear bias/Jacobian convergence while retaining the old, tighter
linear LU-only bounds. The existing default-tolerance diode sweep alone uses
C's default 1e-3 RELTOL + 1e-12 floor: its final stored current differs from the
physical Rust root by 0.062%, not by a new equation approximation. Independent
junction/KCL tests require the Rust point to satisfy the physical law at
1e-8 relative + 1e-12 A and explain the legacy-data error. Goldens were not moved
to make that test pass.

Transient: existing `compare::TRAN` unchanged (1e-3 relative; 1 uV / 1 pA floors),
shared physical times and one-sided breakpoint limits, never matched step arrays.
Worst fractions of the bound in the validated run: diode 0.105, BJT 0.002,
MOS1 0.187.
Each new transient compares 96 physical instants and ten breakpoint limits.

C's default saved signals omit simulator-created internal nodes: verification
projects only known `NodeKind::Internal` columns from the Rust plot, then retains
exact external variable-set/unit checks. The public Rust `sweep` scale name is
normalized to C's `v(v-sweep)`/`i(i-sweep)` naming for verification only.
No model quantities or external signals are excluded to hide an error.

`tests/m4_gate.rs` additionally pins polarity and finite gain,
MOS cutoff/triode/saturation/reversal square law, transistor DC Jacobian finite
differences, diode junction/KCL residuals, physical integrated charge at a finer
mesh, every state-pair's Q-based companion recurrence, discarded-trial/history
isolation, explicit unsupported physics, typed nested sweeps and unchanged
source values, parser fixed points, gmin rescue of a singular initial Jacobian,
source-stepping rescue of an iteration budget and rejection of a regularized
nonunique final circuit. Existing M1/M3, solver and accept-failure regressions
remain required. This is local evidence for the demonstrated subset, not a remote
issue closure or a claim about unsupported physical regimes.

## Validation

Final local run: **587 passed / 29 opt-in tests ignored**, independently on stable
and Rust 1.89.0; all-target Clippy with warnings denied passes on both. All twelve
M4 physical/continuation/ownership tests pass. Formatting and diff whitespace
checks pass; 110 snapshots are unchanged; Rust verified 25/26 goldens at this
M4 gate (only subcircuit flattening was excluded; #18 has since closed that gap
and the current gate verifies all 26), and the external C drift check
reproduces all 26 golden files. The six newly captured M4 goldens are included with the
implementation for review. Remote #41 remains open pending review; unsupported
physics remains outside this local gate.

Run inside this worktree:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo +1.89.0 test --workspace --locked
cargo +1.89.0 clippy --workspace --all-targets --locked -- -D warnings
cargo xtask golden verify
cargo xtask snapshots
```

C drift checks (no fixture recapture):

```sh
NGSPICE_BIN="$PWD/../../ngspice_test/build/src/ngspice" cargo xtask golden check
```
