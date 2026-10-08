# ADR #29: bounded higher-index source-constraint formulation

Status: **numeric prototype for review; production enablement declined**.
Prerequisite #28 is merged (base `803fcb5`). This ADR does not enable a deck,
change `LinearDae`/analysis initialization, or claim general DAE support.

## Context and classification

Production #28 splits `E x' + A x = b` into range and nullspace coordinates,
then requires nonsingular `W^T A N`. A capacitor/ideal-voltage-source loop or
inductor/ideal-current-source cutset can make this block singular even when the
full pencil is regular. A zero residual is not evidence of unique currents or
voltages. Such source constraints typically give linear MNA differentiation
index two; additional degeneracy can instead make the pencil singular.

- **CV loops:** prescribed combinations of capacitor voltages impose charge
  constraints. Differentiating the voltage constraint supplies capacitor current
  and hence voltage-source branch current. Another derivative is needed for the
  source-current derivative. Floating capacitor loops also require independent
  common-mode constraints; graph connectivity alone proves none of these ranks.
- **LI cutsets (dual):** KCL fixes combinations of inductor currents. Its
  derivative supplies flux-rate/inductor voltage and possibly current-source
  voltage. Free modes must still be evolved; reconstructing all branch variables
  requires independent differentiated constraints. This prototype does **not**
  implement the dual class.
- **Redundancy/inconsistency:** dependent source constraints with compatible
  forcing can leave source currents nonunique; incompatible forcing has no
  solution at all. Both must be refused, including homogeneous systems. Structural
  rank (a full matching/incidence topology) is necessary but does not certify
  numeric rank for weighted or cancellation-prone matrices. The assembled numeric
  reduction must separately certify uniqueness.

Read-only upstream behavioral references are `CAPload` in
`src/spicelib/devices/cap/capload.c` (charge `C*v`, `NIintegrate`, signed RHS),
`INDload` in `src/spicelib/devices/ind/indload.c` (flux `L*i`, branch equation,
`NIintegrate`), and `DCtran` in `src/spicelib/analysis/dctran.c` (initialization,
breakpoints and accepted integration history). The new code is an independent
continuous-equation reduction, **not** a translation of companion stamps or C's
trap/Gear timestep policy. No C algorithm or KLU implementation is copied.

## Decision: fully voltage-constrained capacitive block plus free RL currents

Explicit opt-in API: `spice_maths::diffsol::higher_index::ConstrainedPencil`.
Input is immutable numeric `A,E`, coordinate counts `n,k`, and explicit
`ProjectionOptions`; it is not a topology detector or device callback. Coordinates
and original rows are ordered `[q,z,lambda]`, with **ground omitted**:

```text
C q' + G q + B z + H lambda = f       (n nodal KCL rows)
-L z' + B^T q - R z         = g       (k inductor KVL rows)
H^T q                      = s       (n voltage-source rows)
```

`n,k >= 1`, `2*n+k <= 32`; `C,L` are strictly positive diagonal; `H` is square
and numerically nonsingular. Other mass entries and branch/constraint couplings
are exactly zero, and the incidence blocks are exactly transposes. Finite numeric
`G,R,B,H` are allowed; no passivity or physical-incidence certification is claimed
for arbitrary caller-supplied matrices. A physical example is grounded capacitors
on all voltage nodes, a spanning independent grounded voltage-source tree,
resistors, and uncoupled RL branches. A single capacitor/resistor/inductor in
parallel with a prescribed smooth voltage is the smallest acceptance example.
Partial voltage constraints, floating/coupled mass, mutual inductors, nonlinear
charge, switched topology and LI cutsets remain outside this prototype.

Each incidence column's positive current flows from first terminal to second;
`lambda` is the source current, **not its negative**. For a source from node to
ground: `lambda = f - C q' - G q - B z`, negative when delivering positive
capacitor/resistor/inductor current. Node IDs, device IDs, matrix rows and these
local coordinate offsets are distinct namespaces. Production row mapping is
not implemented here.

## Reduction, reconstruction and hidden equations

Free coordinates are the physical inductor currents `z` (dimension `k`), not the
rank of E (`n+k`). All capacitor voltages are fixed by the source constraints.
With explicit forcing data `ForcingJet = (b,b',s'')`, solve:

```text
H^T q   = s
H^T q'  = s'
H^T q'' = s''
L z'    = B^T q - R z - g
H lambda = f - C q' - G q - B z
L z''   = B^T q' - R z' - g'
H lambda' = f' - C q'' - G q' - B z'
```

These are real numeric solves through existing owned dense LU, not a hard-coded
analytic solution. `new` validates/snapshots assembled operators and factors
`H,H^T,L`. `reconstruct(z,jet)` returns physical state, every physical derivative
(including source currents), and acceleration of `[q,z]`. It then certifies
`E x'+A x-b`, `E x''+A x'-b'` and `H^T q''-s''` in **original units** before
returning. Source-current second derivatives are not computed or claimed; E
annihilates those coordinates, so they are unnecessary for these residuals.
`residuals` also certifies independently supplied candidates and exposes vectors
and the maximum residual/bound ratio.

The prototype does not certify that separate supplied jets belong to one actual
waveform: coherent analytic derivatives on a C2 interval are the caller contract.
It never stamps fallibly inside a diffsol callback and runs no time integrator.
An independent test-local RK4 uses the reduced RHS only, not a production runner.

## Initial conditions, uniqueness and source events

`reconstruct` deliberately selects the constrained capacitor charge from the
forcing and preserves supplied `z` exactly. It is **not** the charge-preserving
index-one event projection. `consistent_initial(x,jet)` instead validates a
supplied full physical IC against the unique reconstruction, with physical
per-component bounds. Incompatible voltages/charges or branch source currents
are errors; no large change is silently accepted. A consistent state and
consistent derivatives are returned. Free inductor-current ICs need not be DC.
This numeric operation does not implement `.ic`, `ic=`, `.nodeset` or `uic`.

Square singular H is rejected at preparation, before any RHS is evaluated,
including zero RHS. This conservatively refuses **both** redundant compatible
and inconsistent source sets. It does not classify a given singular set by an
augmented-rank RHS test or choose a gauge to invent unique currents. Tests pin
this refusal and separately check incompatible ICs on a nonsingular pencil.
Nearly dependent H and numerically unresolved L are also refused by the existing
LU rank guard (`epsilon * dimension * max_abs(matrix)` pivot threshold). No rank
or residual tolerance is weakened, and no pseudoinverse selects nonunique lambda.

A constrained-voltage jump `delta q = H^-T delta s` changes charge `C delta q`;
KCL requires a source-current impulse with integrated source contribution
`H integral(lambda dt) = -C delta q`. A current-source jump in the dual LI class
would analogously require a voltage impulse changing flux. These cannot be
represented as ordinary finite continuous branch values. `check_smooth_join`
rejects **any exact** voltage-constraint change, even below numerical tolerance,
with an impulsive-jump error. It also refuses differing forcing values, first
derivatives or constraint curvature at joins. A continuous voltage with a slope
corner need not create a charge impulse, but does create a finite source-current
jump and is outside this C2-only prototype. There is no event acceptance, history
restart, jump interpolation or state commitment.

## Numeric and resource policy

- Reject empty/mismatched/overflowed dimensions before dense allocation; budget
  is explicit, positive, <=32 and enforced on original unknown count.
- Assemble duplicate stamps into owned dense snapshots; reject nonfinite input
  and duplicate-sum overflow. No symbolic cache or new dependency is added.
- Reject vector/jet dimension and finite failures; propagate checked LU errors.
- `0 < relative <= 1e-6`, finite positive physical-state and equation absolute
  bounds with one entry per original unknown/row. No tolerance defaults are
  silently substituted. Tests use relative/absolute `1e-12`.
- Each original/differentiated row satisfies
  `|residual| <= equation_absolute[row] + relative *
  (sum |A[row,c]*x[c]| + sum |E[row,c]*x'[c]| + |b[row]|)`;
  differentiated rows substitute derivatives. Second-constraint rows use the
  analogous `H^T q''` terms. Absolute rows have A/V, A/s or V/s units as applicable;
  the current API uses the same numerical absolute floor for a row and its
  derivative, not a new production time-dependent error-control policy.
- Nonfinite output/scale/bound/residual arithmetic is an error, not success from
  `inf <= inf`. Original-unit residual checks reject corrupted state, derivative
  and acceleration separately. Backward residuals do not promise arbitrary
  forward accuracy or conditioning; highly scaled pencils can be refused.

## Acceptance and capability gate

Source-named tests: `crates/spice-maths/tests/higher_index_prototype.rs`.
Sine and ramp constraints check signed source currents, free RL dynamics,
derivatives, original/differentiated/hidden residuals and IC handling. A two-node
source tree tests coupled constraint coordinates and opposite terminal signs.
Failure paths cover rank/redundancy/inconsistency refusal, homogeneous singularity,
zero mass, impulses/slope corners, unsupported block structure, dimension/finite/
duplicate overflow, residual corruption, budgets and tolerance validation. The
existing six index-one `dae.rs` tests are unchanged, and a new test proves the
same higher-index physical pencil still fails `LinearDae::new`.

**Not production-enabled.** Numeric formulation acceptance can close the bounded
#29 gate after independent review, without claiming general higher-index DAE
simulation. This ADR is a concrete formulation and evidence gate only.

## Narrow enabling follow-ups (not implemented by this prototype)

These are separately tracked prerequisites, not a declaration of production
higher-index support:

- [#69](https://github.com/michaelnavazhylau/ngspice-rs/issues/69): waveform jets
- [#70](https://github.com/michaelnavazhylau/ngspice-rs/issues/70): exact-class adapter
- [#71](https://github.com/michaelnavazhylau/ngspice-rs/issues/71): reduced integration
- [#72](https://github.com/michaelnavazhylau/ngspice-rs/issues/72): nonimpulsive corners

1. **Analytic waveform jet contract, no new deck grammar.** Add device-owned
   fallible value/first/second derivative evaluation for Constant and interior
   affine PWL/PULSE intervals, with explicit left/right derivative metadata and
   refusal at corners/jumps. Validate smooth ramp/sine numeric APIs and event
   budgets; keep unsupported SIN deck grammar unchanged. This removes the current
   unverified supplied-jet caller contract before runtime use.
2. **Opt-in topology-to-pencil adapter for the exact class above.** Recognize
   independent grounded capacitors, free uncoupled RL branches and a full-rank
   voltage-source constraint tree; map physical node/branch rows without mixing
   namespaces. Preserve signed plot currents, validate numeric ranks after
   assembly and reject partial/floating/LI/redundant cases. Add deck-level
   analytic and ignored supported-syntax C ramp comparisons. No default enabling.
3. **Opt-in reduced ODE integration and reconstructed error control.** Integrate
   only z, evaluate immutable analytic jets outside fallible solver callbacks,
   reconstruct physical q/lambda and their derivatives, certify original rows,
   and enforce voltage/current tolerances on *reconstructed* quantities (not just
   z). Cover rejection/work/progress/max-step/final-time checks, full IC rules,
   exact accepted-vs-trial device ownership, and sample-vs-accepted-state behavior.
   Publish no failed numerical trial. Only after review of this slice consider
   an explicit analysis backend opt-in; default index-one guards remain unchanged.
4. **Nonimpulsive source corners, separately gated.** Land on every knot, prove
   continuity of charges/fluxes, reconstruct left/right source-current jumps,
   restart solver history and accept only verified event states. Never interpolate
   across the event; keep actual voltage jumps unsupported. Require common-grid
   analytic/C evidence before broadening the smooth-only class. Dual LI cutsets
   and partial-constraint reductions require separate ADR/tests, not this enabling
   issue's implicit scope.
