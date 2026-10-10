# Typed scalar DC sweeps (#35, #97)

Scope: the bounded `.dc` driver in `analysis::sweep` (C: `dctrcurv.c`).
It sweeps scalar targets over one inner axis and one optional outer axis. It is
not a model-parameter sweep (C has none, see below) or a `.step` analysis.
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
| `@inst[param]` | the instance (any device) and its settable real parameter | that instance parameter (#97) | `parameter` |

Targets are identified by what the device *is*, never by the first letter of its
name: a programmatic resistor named `v9` is a resistor target, a voltage source
named `r9` is a voltage-source target, and a capacitor named `rc` is rejected.
Lookup is case-insensitive and the plot/`SweepTarget` keep the instance's own
spelling. `temp` wins over a device that is literally named `temp`.

Anything else (a bare capacitor, inductor, diode, BJT or MOSFET name, model
names, `r1.r`-style references, nodes, more than two axes, malformed arity, a
duplicate quantity in the two axes) is `Unsupported` and is rejected **before
the first sample**, as is every invalid grid or value below. Instance
parameters are described in [Instance-parameter targets](#instance-parameter-targets-97).

## What C sweeps (#97)

Established by reading `dctrcurv.c`, `dctsetp.c`, `inp2dot.c` (`dot_dc`) and
`trcvdefs.h`, and by running the reference binary on probe decks (results
below are its output; nothing in the C tree was changed):

- **Two nesting levels, no more.** `TRCVNESTLEVEL` is 2 and `dot_dc` reads
  `name1 start1 stop1 step1 [name2 start2 stop2 step2]`. Anything after the
  eighth token is **silently ignored**: `.dc v1 1 2 1 r1 1k 2k 1k temp 27 28 1`
  runs a two-level sweep without the `temp` axis. The port rejects a third axis
  (and any other arity) instead of dropping it.
- **Target kinds.** `DCtrCurv` resolves each name, in order, as a resistor
  (`RESname`, sweeping `RESresist`), voltage source, current source, the word
  `temp`, and finally `@instance[parameter]` (`DCTfindInstParam`, the
  "Enhancement-62" `PARAM_CODE` path present in the reference build). Anything
  else is fatal: `Voltage source, current source, or resistor named "..." is
  not in the circuit`.
- **Instance parameters only.** `DCTfindInstParam` searches every device type's
  *instances* for the name (case-insensitive) and then only that device's
  instance table for a keyword with `IF_SET` and `IF_REAL` (aliases included,
  e.g. diode `perim` for `pj`, current source `c` for `dc`). It never looks at
  model tables: `.dc @dm[is] ...`, `.dc dm[is] ...`, `.dc @d1[is] ...` (IS is a
  model parameter) and `.dc @d1[foo] ...` all fail with the error above. A
  `.param` name (`.dc pp 1 3 1`) fails the same way; `inpcom.c` only repairs
  `.dc (TEMPER)` back to `.dc TEMP` and does not rewrite parameter sweeps.
- **Setting a parameter.** Each point calls the device's `DEVparam` setter with
  the new value (marking it given) and then `DEVtemperature` for the device
  type (`DCTsetInstParam`); nothing reruns `DEVsetup`. Consequences that the
  port reproduces: a G/F gain set this way is multiplied by an `m` given
  anywhere on the card (`VCCSparam`/`CCCSparam`), a G/F `m` sweep has no effect,
  and a BJT `area` sweep leaves AREAB/AREAC at the values `bjtsetup.c` copied
  from the card's AREA. The value is accumulated as `now += step` with C's
  absolute `1e3 DBL_EPSILON` stop test, like sources.
- **Restoration quirk.** After the sweep C restores the saved value but cannot
  clear the "given" flag, so a later analysis in the same run sees, e.g., a
  diode TEMP fixed at the temperature it had during the sweep. The port never
  mutates devices and has no such carry-over.
- **Same quantity twice.** C accepts `@d1[area]` in both levels, or `r1` and
  `@r1[r]`, and produces order-dependent results; the port rejects both.
- **Vector naming.** The scale of the inner axis is `v-sweep`/`i-sweep`/
  `temp-sweep`/`res-sweep`/`param-sweep`; the rawfile writes `param-sweep` as
  `v(param-sweep)` of type voltage and `res-sweep` with type `res-sweep`. C
  writes no column for the outer value. The port's public plot keeps its
  `sweep` scale (unit `parameter` for `@inst[param]`) and its `sweep(<outer>)`
  column; `golden verify` maps both conventions.
- **Accuracy.** At C's default `reltol=1e-3` the warm-started points stop up to
  ~3e-4 V from the root of a diode sweep; with `.options reltol=1e-8` C and the
  port agree to well under 1 ppm, so every nonlinear comparison sets it.

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
([SWITCHES.md](SWITCHES.md)) and junction devices limit against the previous
point's voltages. Under the default ngspice continuation every point after the
first is first solved by a plain warm-started Newton bounded by C's effective
`itl2` (100), falling back to the full `CKTop` sequence on a numerical failure
(`dctrcurv.c`; #106), which is what lets Schmitt triggers follow different
branches up and down (`m7_conv_*_schmitt` goldens). In a nested sweep the first point of every inner
sweep restarts like the very first point (`dctrcurv.c` `firstTime`). Circuits
with discrete-state devices sweep C's accumulated values
(`SweepSpec::accumulated_grid`: `value += step`, absolute `1e3 DBL_EPSILON`
stop test, temperature accumulated in kelvin) instead of the grid above,
because a switch control on a threshold is decided by the last bit.

## Instance-parameter targets (#97)

`@instance[parameter]` is split into a nonempty instance and parameter (text
after `]`, which C ignores, is rejected). Lookup is case-insensitive; the
target keeps the instance's spelling and the canonical keyword
(`@d1[pj]` for `@D1[PERIM]`). Each target takes one of four routes
(`ParameterRoute`):

| Written | Route | Applied as |
| --- | --- | --- |
| `@v1[dc]` | `VoltageSource` | the V-source target (factor reuse kept) |
| `@i1[dc]`, `@i1[c]` | `CurrentSource` | the I-source target |
| `@r1[r]`, `@r1[resistance]` | `Resistor` | the resistor target (`RESparam` + `REStemp` equal `RESresist` + `RESupdate_conduct`) |
| supported device parameter | `Device` | a per-point device replacement |

Device parameters supported by the port, each re-derived exactly as the
device's temperature routine does:

| Device | Parameters | C reference |
| --- | --- | --- |
| diode | `area`, `pj` (`perim`), `m`, `temp`, `dtemp` | `dioparam.c`, `diotemp.c` |
| Gummel-Poon BJT | `area`, `areab`, `areac`, `m`, `temp`, `dtemp` | `bjtparam.c`, `bjttemp.c` |
| MOS1 | `m`, `l`, `w`, `ad`, `as`, `pd`, `ps`, `nrd`, `nrs`, `temp`, `dtemp` | `mos1par.c`, `mos1temp.c` |
| E/F/G/H | `gain` (G/F times a given `m`) | `vcvspar.c`, `cccspar.c`, `vccspar.c`, `ccvspar.c` |

The remaining M8 real setters are implemented through immutable per-point
replacements: R `temp`/`tc1`/`tc2`/`w`/`l`/`m`/`scale`, C/L values, V/I
`acmag`/`acphase` and I `m`, D `ic`/`w`/`l`, Q `icvbe`/`icvce`, MOS1
`icvds`/`icvgs`/`icvbs`, G/F `m`, B `temp`/`dtemp`/`tc1`/`tc2`/`m`, K `k`.
Setter effects follow DEVparam then DEVtemperature, without rerunning setup:
D geometry and Q setup-only area coefficients retain C's already-resolved
values; G/F `m` updates the stored multiplier but not the gain until a gain
setter runs. K and swept inductances feed the mutual-inductance assembly.
Unknown parameters remain explicit errors. Live C tests cover these setters
and the earlier MOS1 geometry/temperature, Q AREAC/TEMP and D PJ setters.

Values are checked against the instance schema domains (AREA/M/W/L positive,
PJ/AD/AS/PD/PS/NRD/NRS nonnegative, TEMP above absolute zero) and the device's
own construction checks (diode sidewall/TM1 restrictions, `L - 2 LD > 0`,
MOS1 RSH/NRD conductances). A diode DTEMP sweep on an instance with TEMP is
rejected, as the card-level rule rejects TEMP with DTEMP (C silently ignores
DTEMP there); a MOS1 or BJT DTEMP sweep under an instance TEMP has no effect in
either simulator. MOS1 values that would create or remove an internal
drain/source node are rejected; nothing re-runs the topology.

Immutability follows the resistor design: `Circuit::instance_override(name,
keyword, value, context)` validates by building the replacement once and
returns an `InstanceOverride` (device ordinal, canonical keyword, value);
`ModelContext::instance_overrides` carries at most `MAX_INSTANCE_OVERRIDES` = 2,
two parameters of one device are allowed, the same parameter twice is not.
`Circuit::load`, `linear_system_with_context` and `small_signal_system` stamp
`Device::with_instance_parameter(...)` — a disposable copy with the same
terminals, branch rows and state layout — in place of the original for that
point. A temperature axis is applied before the instance parameters of the
same point. Every Cartesian point of a sweep with a device-parameter or
temperature axis is assembled once before the first sample, so an invalid
value anywhere in the grid is an up-front error.

Tests: `tests/dc_parameter_sweeps.rs` (diode junction law over AREA, AREA/M
equivalence, instance TEMP/DTEMP versus circuit `temp`, E/F/G gains with `m`,
source/resistor routes equal the typed targets, BJT/MOS1 identity and W/L/M
scaling, routing/canonical names, rejections, immutability, override bounds).
C goldens `m8_dc_param_diode` (AREA x circuit TEMP), `m8_dc_param_mos1`
(W x L with RSH series nodes), `m8_dc_param_gain` (G gain with `m` x E gain)
and `m8_dc_res_temp` (resistor `res-sweep` scale) are in `golden verify`, and
the opt-in `c_dc_param_sweep_reference` compares 15 more decks (diode
AREA/PERIM/M/TEMP/DTEMP with temperature and source axes, BJT AREA/AREAB/M/DTEMP,
MOS1 W/L/M/NRD/TEMP, E/F/G/H gains, `@v1[dc]`/`@i1[c]`/`@r1[r]`/`@r1[resistance]`)
against the live reference binary.

## Limits and non-goals

Model-parameter targets (`@rm[r]`, `@dm[is]`, `rm.r`) are not sweepable in C
and stay `Unsupported`; neither are `.param` names. More than two axes are
rejected (C silently drops them). Instance parameters outside the table above
and list/decade/octave sweeps are unsupported. Instances inside subcircuits are
named as C flattens them (`@d.x1.d1[area]`). Two resistor axes are allowed; the same quantity twice is
not. Convergence policy (`gmin`/source stepping, itl
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
