# Typed scalar DC sweeps (#35)

Scope: the bounded `.dc` driver in `analysis::sweep` (C: `dctrcurv.c`).
It sweeps scalar targets over one inner axis and one optional outer axis. It is
not a general parameter sweep, a model-parameter sweep or a `.step` analysis.
No new parser exists: the `.dc` card's positional arguments come from the existing
analysis-card AST (so `{expr}`/`.param` arguments already work) and are read as
`target start stop step [target start stop step]`. Named `key=value` arguments
are convergence controls owned by `DcSettings::from_request` and are ignored by axis parsing.

## Targets

| Target | Identified by | Swept quantity | Plot unit |
| --- | --- | --- | --- |
| independent V source | `SourceKind::Voltage` of the assembled source | DC value (V) | `voltage` |
| independent I source | `SourceKind::Current` of the assembled source | DC value (A) | `current` |
| resistor, literal or model-backed | `Device::resistor_metadata()` | **supplied scalar** (ohm) | `resistance` |
| `temp` | the reserved word | circuit temperature (Celsius) | `temperature` |

Targets are identified by what the device *is*, never by the first letter of its
name: a programmatic resistor named `v9` is a resistor target, a voltage source
named `r9` is a voltage-source target, and a capacitor named `rc` is rejected.
Lookup is case-insensitive and the plot/`SweepTarget` keep the instance's own
spelling. `temp` wins over a device that is literally named `temp`.

Anything else (capacitors, inductors, diodes, BJTs, MOSFETs, model names,
`r1.r`-style parameter references, nodes, more than two axes, malformed arity, a
duplicate target in the two axes) is `Unsupported` and is rejected **before the
first sample**, as is every invalid grid or value below.

## Supplied scalar versus effective resistance

The swept value replaces the resistor's **supplied scalar**: the primary
`r1 a b <value>` resistance. The *effective* resistance is recomputed for every
point from it and the point's temperatures:

```text
literal:       Reffective = supplied
model-backed:  Reffective = supplied * (1 + TC1*dT + TC2*dT^2) * scale / m
```

For a model-backed resistor the supplied scalar outranks the model `r` and RSH
geometry (rule 1 of [PASSIVE_MODELS.md](PASSIVE_MODELS.md)), exactly as an explicit
instance value does, so sweeping `r1` replaces model `r` rather than scaling it.
TC1/TC2, instance `TEMP` (which still overrides the swept circuit temperature),
model `TNOM`, `scale` and multiplicity `m` keep acting on the swept value. A
literal resistor has no temperature or multiplicity law.

Read-only C source inspection confirms `dctrcurv.c` replaces the supplied
instance resistance (`RESresist`, marked given), and `restemp.c` applies
TC, scale and multiplicity. The opt-in live comparisons below validate this
bounded mapping against the external reference binary. Tests also pin the
Rust semantics analytically (`tests/dc_sweeps.rs`,
`tests/resistor_overrides.rs`): scale 2, `m` 4, TC1 0.01, 47 C
against TNOM 27 C gives `Reffective = 0.6 * supplied`; at 27 C it is
`0.5 * supplied`; a fixed instance `temp=77` does not move with the sweep.
Negative nonzero scalars are legal (as for literal instances); zero, nonfinite
values and values with nonfinite conductance are not.

## Immutable per-point override

Nothing is mutated and nothing is rolled back. `devices::sweep` defines:

- `Device::resistor_metadata()` / `Device::resistor_effective(supplied, ctx)` -
  default `None` / error; implemented by the literal `Resistor` and by the
  model-backed resistor wrapper (family `R` only, never C/L).
- `ResistorOverride`, made by `Circuit::resistor_override(name, supplied)`
  (physical lookup, scalar validation) and carried in the immutable per-point
  `ModelContext::resistor_overrides` (at most `MAX_RESISTOR_OVERRIDES` = 2, no
  duplicate resistor; `ModelContext::with_resistor_override`).
- `Circuit::load`, `linear_system_with_context` and `small_signal_system` build a
  disposable `Resistor` with the override's effective value for the targeted
  device only, and stamp that instead. The circuit's devices, model recipes,
  sources and the netlist AST are never touched, so success, an unsupported
  request and a mid-run convergence failure all leave them exactly as they were.
- `Circuit::effective_resistance(&override, &context)` exposes the same
  computation for validation and tests.

The override names a device by its ordinal in `Circuit::devices()`. Out-of-range
ordinals and non-resistor/non-scalar targets are errors. Overrides are circuit-local,
like node IDs: do not reuse them with a different circuit or after device reordering.
Per-point contexts reach configured `bias::solve_dc_with` through `&ModelContext`.

Sources are overridden as before (temporary RHS offsets), and temperature is the
per-point `ModelContext::temperature`.

## Grids

`SweepSpec::grid` builds `start, start+step, ...` in the sign of `step`:

- **Inclusive reachable endpoint.** The step count is
  `floor(ratio + 32*f64::EPSILON*max(ratio,1))` with `ratio = (stop-start)/step`, and when the
  grid reaches `stop` within that slack the last value is exactly `stop`.
  `0..0.3 by 0.1` is `2.9999999999999996` steps in binary floating point and
  yields four points ending at exactly `0.3` (the earlier floor dropped it).
  A stop not on the grid (`0..1 by 0.4`) is not included and never overshot.
- **Direction.** Forward, reverse and signed grids work (`1..0 by -0.25`,
  `-1..1 by 0.5`); the first axis is the inner (fast) loop, the second the outer.
  `start == stop` is one point for either sign of step.
- **Rejected:** zero (including `-0`) or nonfinite step, nonfinite start/stop,
  a step pointing away from stop, span or point-count overflow, steps that make no
  progress (`1e16..+4 by 0.5`), temperatures at or below absolute zero, resistances
  that are zero or have nonfinite conductance.
- **Bounds.** At most `MAX_SWEEP_POINTS` = 100 000 points per axis and for the
  Cartesian product (checked multiplication). The grid is built once per axis;
  no work happens before every axis has been validated.

## Validation before samples

Besides target/grid checks, the driver rejects input-validity failures that would
otherwise surface mid-run: actual Cartesian R/TEMP point contexts are assembled
and every swept resistance is evaluated over the whole grid. Original resistor
recipes are not evaluated in place of the replacements; an otherwise invalid
original resistance at a swept temperature must not reject valid replacement points. A model
with `tc1=-0.03` swept to 67 C, or `scale=1e300` swept to 1e10 ohm, is rejected
up front with zero accepted points. Convergence failures of nonlinear points
remain run-time errors; they return `Err` and no plot, and device accept hooks
have run only for points that were solved.

## Factor reuse and warm starts

A linear circuit swept over **sources only** assembles and factors the operator
exactly twice - one probe assembly in `resolve`, then one factor reused for every
right-hand side, pinned as an absolute count rather than a comparison. A resistor
or temperature axis changes the operator, so each point re-assembles and re-solves
through configured `bias::solve_dc_with` (tests pin one operator assembly per
additional point, and a 1k/1k divider swept over `r1` must not return 1 V at every
point, which detects a stale factor). Nonlinear circuits keep seeding each Newton
solve with the previous accepted point (and `.nodeset` for the first); that
threading is pinned by a ramp that converges under a per-solve budget too small
for a cold solve of its own final point. Each nonlinear point's converged state
is accepted into a history the next point continues (`dctrcurv.c` rotates its
state vectors), starting in `MODEINITPRED`; only devices with discrete state
read it, so S/W switches keep their hysteresis across a sweep
([SWITCHES.md](SWITCHES.md)). In a nested sweep the first point of every inner
sweep restarts like the very first point (`dctrcurv.c` `firstTime`). Circuits
with discrete-state devices sweep C's accumulated values
(`SweepSpec::accumulated_grid`: `value += step`, absolute `1e3 DBL_EPSILON`
stop test, temperature accumulated in kelvin) instead of the grid above,
because a switch control on a threshold is decided by the last bit.

## Limits and non-goals

Model-parameter targets (`rm.r`), geometry (`l`/`w`), `tc1`/`tc2`, `scale`, `m`,
inductors, capacitors, nonlinear devices, subcircuit instances, list/decade/octave
sweeps and more than two axes are unsupported. Two resistor axes are allowed;
the same resistor twice is not. Convergence policy (`gmin`/source stepping, itl
options) belongs to `bias.rs`/`newton.rs`; sweep requests use the same configured
policy as OP/AC (see [DC_CONTINUATION.md](DC_CONTINUATION.md)).

Tests: `tests/dc_sweeps.rs` (grids, nested V/I, R/TEMP and
mixed source+resistor axes, analytic literal/model/nonlinear points, programmatic
names, warm-seeded ramps, absolute factor-reuse counts, restoration, failure,
rejection that is proven to happen before any solve rather than as a mid-run
numerical failure, limits) and `tests/resistor_overrides.rs`
(metadata, override construction, context capacity, all three assembly/load paths,
invalid ordinals). `dc_followup_regressions` covers configured policy wiring and
actual replacement context validation.

The opt-in `c_dc_sweep_reference` test runs eight temporary C decks: literal
forward/reverse resistance, both model R/TEMP axis orders, fixed instance TEMP,
geometry-derived resistance replaced by a scalar, negative model resistance, and
nested V/I sources. It compares inner scale, voltages and branch current at
`1e-12` relative plus `1e-15` absolute (static linear arithmetic). All eight live
cases passed against the local read-only C build. No new golden was captured;
existing goldens and bounds remain unchanged.

```sh
NGSPICE_BIN=/absolute/path/to/ngspice cargo test -p ngspice-rs \
  --test c_dc_sweep_reference --locked -- --ignored
```
