# Bounded model-backed passives (#19)

This checkout implements #19 on merged prerequisite PR #51 (#11/#17), pending
merge. It extends the existing linear engine; no nonlinear passive/device
physics, new solver backend or full M1/M3/SPICE-parity claim is implied.

## Supported surface

The table is exhaustive: other model/instance setters error, even if a supplied
scalar would make their geometry branch unnecessary. Setters remain raw and
ordered in the AST; each scalar is range-checked, then the last duplicate wins.
The first model declaration wins case-insensitively, as in `INPmakeMod`.

| Family | Model setters | Instance setters |
| --- | --- | --- |
| R (`r`, `res`) | `r`, `rsh`, `defw`, `l`, `narrow`, `short`, `tc1`, `tc2`, `tnom` | `resistance` (parser maps `r`), `l`, `w`, `tc1`, `tc2`, `temp`, `scale`, `m` |
| C | `cap`, `cj`, `cjsw`, `defw`, `narrow`, `short`, `tc1`, `tc2`, `tnom` | `capacitance` (parser maps `c`/`cap`), `l`, `w`, `tc1`, `tc2`, `temp`, `scale`, `m`, `ic` |
| L | `ind`, `tc1`, `tc2`, `tnom` | `inductance` (parser maps `l`), `tc1`, `tc2`, `temp`, `scale`, `m`, `ic` |

`level` is separately consumed by the existing resolver's bounded selector
policy, not ignored as a physics setter; see [MODEL_SCHEMAS.md](MODEL_SCHEMAS.md).
This expanded instance surface requires a declared model. Literal factory/API
support is unchanged (primary scalar and C/L IC only); negative nonzero scalar
instance R remains allowed. Model R and all base/effective C/L must be positive.

Units: R/RSH in ohms (RSH per square); C in farads; L in henries; geometry in
metres; CJ in F/m²; CJSW in F/m; TC1 in K⁻¹; TC2 in K⁻²; TEMP/TNOM input in
Celsius. Scale and multiplicity are positive finite dimensionless scalars, not
integer-only. C IC is volts; L IC is amperes. Under companion `.tran ... uic`
the instance IC seeds the model-backed C/L exactly as a literal one (charge
from the effective, temperature-adjusted capacitance; inductor branch current),
and the model-backed C/L charge/flux takes part in truncation control like the
scalar device it delegates to (#80 review fix; checked against C for coupled
inductors and a TC/`m` capacitor). Without `uic` IC is ignored, as in C.

## Values, geometry and defaults

### Resistor

References: `ressetup.c::RESsetup`, `restemp.c::RESupdate_conduct`.

1. Last explicit instance resistance wins over model/geometry.
2. Otherwise positive RSH selects sheet geometry **ahead of** model R, even
   when L/W are omitted (C supplies default dimensions).
3. Otherwise explicit model R is required; Rust does not use C's warning-and-
   1 mOhm missing-resistance fallback.

```text
L = instance l, else model l, else 10 um
W = instance w, else model defw, else 10 um
Leff = L - 2*short
Weff = W - 2*narrow
Rbase = RSH * Leff/Weff
```

RSH defaults to zero (geometry disabled); short/narrow default to zero. RSH and
corrections must be nonnegative; L/W and effective dimensions must be positive.
The nominal value must be nonzero with representable finite reciprocal. Negative
model R is rejected (C ignores that setter); negative instance R is retained.
Tiny-resistor clamps and global frontend geometry scale are not applied.

### Capacitor

References: `capsetup.c::CAPsetup`, `captemp.c::CAPtemp`.

1. Last explicit instance C wins.
2. Otherwise explicit model CAP wins over geometry.
3. Otherwise require positive instance L, and use instance W or model DEFW
   (default 10 um).

```text
Leff = instance L - short
Weff = W - narrow
Cbase = CJ*Weff*Leff + CJSW*2*(Leff+Weff)
```

CJ/CJSW and corrections default to zero and must be nonnegative; effective
L/W and final C must be positive. **C uses one correction, R uses two.**

C stores model `defl` but does not apply it to instances: `CAPsetup` initializes
missing instance length to zero. The live setup probe exposed this distinction.
Rust therefore rejects `defl` and missing/degenerate L, rather than inventing a
model-length default or accepting C's negative-area/sidewall corner cases.
Dielectric/thickness-derived CJ (`di`, `thick`), `del` and geometry aliases are
not part of this initial surface.

### Inductor

References: `indsetup.c::INDsetup`, `indtemp.c::INDtemp`, `indacld.c::INDacLoad`.
Last explicit instance L wins; otherwise explicit model IND is required.
Coil geometry (`nt`, `csect`, `dia`, `length`, `mu`) needs C's specific-inductance
and Lundin correction behavior and is **explicitly unsupported**, even alongside
an explicit scalar. No invented simple-solenoid approximation is used.
K cards couple model-backed inductors through C's `INDinduct` (temperature- and
scale-adjusted, before `/m`); see [MUTUAL_INDUCTANCE.md](MUTUAL_INDUCTANCE.md).

## Temperature, scale and multiplicity

References: `restemp.c`, `captemp.c`, `indtemp.c`, `capacld.c`, `indacld.c`.

```text
T = instance TEMP, else AnalysisContext.temperature
Tnom = model TNOM, else AnalysisContext.nominal_temperature
DeltaT = (T+273.15) - (Tnom+273.15)
TC1 = instance TC1, else model TC1, else 0
TC2 = instance TC2, else model TC2, else 0
f = 1 + TC1*DeltaT + TC2*DeltaT²
Reffective = Rbase*f*scale/m
Ceffective = Cbase*f*scale*m
Leffective = Lbase*f*scale/m
```

Scale/M default to 1. TC1/TC2 are independent per-coefficient overrides; explicit
zero overrides a nonzero model coefficient. TEMP does not change TNOM. R uses
C's Horner polynomial evaluation order; C/L use the direct sum. All temperatures
must be finite and above absolute zero; factors must be positive and finite.
Derived overflow, zero effective values and nonfinite conductance error explicitly;
no clamps or cumulative correction can conceal them.

`CAPask` includes M in its capacitance query; `INDask` omits M in its inductance
query. The oracle divides C's queried inductance by M before comparing the value
actually stamped. R's queried resistance is unadjusted; with a 1 V drive its
queried current verifies effective conductance instead.

## Production APIs and atomicity

- `ResolvedModel::passive_parameters(&instance)` returns immutable typed
  `PassiveParameters`: `family`, `nominal_value`, `multiplicity`,
  `initial_condition`, and `effective_value(&ModelContext)`.
- `Circuit::from_netlist` validates at 27 Celsius. For a nondefault construction
  temperature use `from_netlist_with_context(&netlist, &context.model_context())`.
- All four analysis drivers copy `AnalysisContext` settings via `model_context`
  into `Circuit::linear_system_with_context`. Default `linear_system()` remains
  the 27 Celsius convenience API.
- `LinearContext` carries explicit `ModelContext`; `StampContext` now carries
  both circuit and nominal temperatures. The wrapper derives scalar R/C/L and
  delegates to existing stamps. Recipes are immutable across hot/cold/repeated
  analyses; factors, node IDs, branch order/signs and accepted state are preserved.
- Schema/geometry/constructor failures leave the caller's circuit/node table
  unchanged. Unused model declarations still error rather than discarding physics;
  duplicate shadowed declarations follow the documented first-definition policy.

Example through the analysis API:

```rust
let context = analysis::AnalysisContext {
    temperature: 77.0,
    nominal_temperature: 27.0,
};
let mut circuit = devices::Circuit::from_netlist_with_context(
    &netlist, &context.model_context(),
)?;
let plot = analysis::runner(request.kind)?.run(&mut circuit, &request, &context)?;
```

## Explicit limits and verification

Behavioral/nonlinear passives, advanced resistor levels, coil geometry, DTEMP,
TCE/exponential temperature, AC-only resistance, noise/breakdown fields, unsupported
aliases, global geometry scaling and `.option` parsing remain gaps. D/Q/M factories,
Model-backed C/L delegate to the scalar trap/Gear-2 companion stamps, which have
no transient driver yet; general DAEs remain unavailable.

Owner-crate tests check formulas, precedence, duplicates, signs, missing/invalid
geometry, overflow, immutable recipes and atomic failures. Production tests compare
literal/model DC, source sweeps and complex AC; a contextual RC step matches its
analytic BDF transient bound. Live C tests check effective values and DC/AC responses
at default and nondefault TEMP/TNOM without changing committed goldens. They also
exposed an existing logarithmic-AC integer-span roundoff bug; a bounded arithmetic
snap preserves the endpoint, with a regression for integer/noninteger spans and
no change to value-comparison tolerances. See
[VERIFICATION.md](VERIFICATION.md#bounded-passive-elaboration-verification) for
commands, counts and bounds.
