# Passive syntax and initial model infrastructure (#11 / #17)

These prerequisites are implemented in the current checkout and pending merge.
They do **not** implement #19 model-backed passive arithmetic or D/Q/M
simulation. The parser still accepts six of the eight rawfile fixture decks;
`golden verify` still verifies three linear fixtures and excludes five.

## Bounded passive syntax

`parser/linear.rs` uses the existing deck-local declaration index, including
forward references before `.end`, excluding unsupported nested/control scopes.
It does not check model family or evaluate geometry/defaults. Examples:

```spice
r1 a 0 1k                  ; ordinary literal (unchanged)
r2 a 0 rm                  ; retained model, no explicit resistance
r3 a 0 1k rm resistance=2k  ; pre-model scalar then named setter: last is 2k
r4 a 0 rm 1k resistance=2k  ; post-model scalar applied last: last is 1k
r5 a 0 rm l=10u w=2u       ; retained geometry, not evaluated
.model rm r(rsh=100)
```

C/L follow the same value/model ordering. Declared bare keyword-like names can
be model references; `keyword=value` stays an assignment. An initial numeric
literal stays a scalar even if a model has that name. Numeric-looking passive
model references **after a scalar** remain explicit gaps: live C probing of
`r1 a 0 1k 123` did not prove model semantics (C reported resistance 123).
Unknown names, expressions, flags, malformed/trailing tokens and overflowing
scalars never result in a partial successful AST. Model-less R/C/L still require
an explicit scalar. No AST parameter/default is synthesized for omitted values.

References: `inp2r.c`, `inp2c.c`, `inp2l.c` and ordered `INPdevSet` calls.
The Rust-only tree contains references, not copies of those C sources.

## Resolver and typed APIs

Public interfaces live in `spice-devices`, not the parser or maths crates:

- `ModelResolver::new(&netlist.models)` indexes one top-level deck without global
  state. Names are case-insensitive; the **first declaration wins**, as in
  `inpmkmod.c::INPmakeMod`. Shadowed cards stay visible in the AST.
- `resolve(&instance)` returns `Option<ResolvedModel>` (literal devices need no
  model), reports missing D/Q/M references and wrong families, and validates
  selectors. Model and node namespaces remain separate: `gnd`, `0` and numeric
  model names are not rewritten by lookup. C's broad ground-name preprocessing
  is a deliberate, unproven collision divergence, not a parity claim.
- `ResolvedModel::{card,family,levels}` expose immutable raw declaration and
  validated identity/selection; successful resolution does **not** validate all
  physics keywords or assert a simulation backend exists.
- `diode_parameters(&ModelContext)` and
  `diode_instance_parameters(&instance, &ModelContext)` supply bounded typed
  input records. They do not construct or stamp a diode.
- `ScalarSchema::validate(assignments, location)` is the schema extension point:
  canonical names, units, domains and optional defaults. It checks **every**
  ordered setter (invalid earlier values cannot be hidden by a later valid one),
  rejects unknown names/nonliteral/nonfinite values and retains the last value
  plus its source location. Defaults have no setter location. `get` is
  case-insensitive. The original AST is never mutated.
- `ResolvedModel::parameters(&schema)` validates a device-owned model schema,
  excluding `level` already consumed by the resolver. Future passive/BJT/MOS
  owners add their own schemas/typed conversions; they must not claim physics
  from parser lookup or generic validation alone.

Example (also covered by module rustdoc and production-interface tests):

```rust
let resolver = spice_devices::ModelResolver::new(&netlist.models)?;
let model = resolver.resolve(&netlist.devices[0])?.expect("diode reference");
let context = spice_devices::ModelContext::default(); // TEMP/TNOM = 27 Celsius
let model_inputs = model.diode_parameters(&context)?;
let instance_inputs = model.diode_instance_parameters(&netlist.devices[0], &context)?;
// Both records are validated inputs, not an available simulation factory.
```

`ModelContext` avoids a lower-crate dependency on `spice-analysis`. Callers may
explicitly pass circuit and nominal temperatures; netlist `.option` processing
is still unsupported. All temperatures must be finite and exceed absolute zero.

## Raw levels, backend selection and applied setters

`ModelCard.level` remains the first **raw** scalar; ordered assignments retain all
occurrences. The resolver verifies that raw cache rather than rewriting it.
Integer conversion is `floor(raw + 0.5)`, following `inpgval.c`. Every level must
be finite, nonnegative and round into 0..=99; Rust rejects C warning/fallback or
unsafe-cast cases instead of silently changing the input.

| Family | Backend selection | Initial supported level policy |
| --- | --- | --- |
| BJT NPN/PNP | First explicit rounded level; default 1 | 0/1/2 (classic BJT); other selectors fail |
| MOS NMOS/PMOS | First explicit rounded level; default 1 | MOS1 only (1); other selectors fail |
| `r` | First explicit rounded level; default 1 | Scalar resistor selector 0/1; advanced selectors fail |
| `res`, `c`, `l` | C fixes backend 1 without scanning level | Bounded Rust contract requires first explicit level to round to 1 |
| `d` | C fixes backend 1 without scanning level | Last ordered integer setter applies; only final applied level 1 supported |

BJT/MOS/passive `level` is selector-only in the initial C tables: later valid
level assignments do not change the backend and are not model data setters.
Diode **does** have an integer model setter. Thus diode
`level=3 level=1.49` has first raw 3, fixed selector 1 and final applied level 1;
MOS `level=1.49 level=49` selects MOS1, with no applied level field. This separation
is explicit in `LevelSelection::{first_raw,selector,applied}`. C would ignore some
passive level values that Rust conservatively rejects. Unsupported levels return
located errors with C references; recognition of classic BJT/MOS1 does not
make their factories available.

References: `inpfindl.c::INPfindLev`, `inpdomod.c::INPdomodel`, `inpgmod.c`,
`inpgval.c` and `diompar.c::DIOmParam`.

## Initial diode inputs

| Input | Domain / unit | Default |
| --- | --- | --- |
| Model IS | Strictly positive, finite A | 1e-14 |
| Model N | Strictly positive, finite dimensionless | 1 |
| Model RS | Nonnegative, finite ohm | 0 |
| Model TNOM | Finite Celsius, Kelvin > 0 | Context nominal temperature (27 Celsius) |
| Instance AREA | Strictly positive, finite dimensionless | 1 |
| Instance TEMP | Finite Celsius, Kelvin > 0 | Context circuit temperature (27 Celsius) |

Typed temperatures are Kelvin (`Celsius + 273.15`). TEMP and TNOM are independent;
explicit model TNOM does not set instance TEMP. These defaults/conversions follow
`diosetup.c`, `diompar.c`, `dioparam.c` and `diotemp.c`. Input ranges are deliberate
fail-closed Rust validation, not a claim that C rejects the same values.

Other keywords/aliases (including JS, capacitance, DTEMP, M, geometry, IC and
flags) are explicit gaps. Numerical setup, IS epsmin clamping, compatibility-mode
RS substitution, temperature-dependent derived quantities and nonlinear equations
remain device work, not hidden schema corrections.

## Circuit boundary and tests

`Circuit::add_instance` takes a resolver/context, validates before construction
and stages changes. On any error, existing node IDs, device ordinals and branch
rows remain unchanged. Successful scalar insertion still requires `finalize`
to rebuild numbering. `Circuit::from_netlist` uses this path with default context.
Model-backed passive and D/Q/M factories stay unavailable; valid diode inputs
still end in `NotYetPorted`. Unused model declarations remain explicit unsupported
inputs instead of disappearing from a successful scalar simulation. Registry
ported flags are unchanged.

Ordinary tests: `spice-netlist/tests/passive_models.rs` and
`spice-devices/tests/models.rs` cover syntax, scoping/order, raw AST preservation,
missing/wrong models, namespace collisions, rounding/cache/range failures,
defaults, provenance, invalid context and atomic failure. They need no C binary.

Opt-in probes (unique temporary directories, no FFI or golden writes):

```sh
NGSPICE_BIN=/absolute/path/to/ngspice cargo test -p spice-netlist --test c_reference --locked -- --ignored
NGSPICE_BIN=/absolute/path/to/ngspice cargo test -p spice-devices --test model_c_reference --locked -- --ignored
```

`passive_models.cir` pins setter order and geometry-based C setup values.
`model_schemas.cir` pins first declarations, default/explicit diode IS/N/RS/AREA/
TEMP/TNOM, repeated diode integer levels and first BJT/MOS selector behavior.
These are setup/input checks, not Rust nonlinear simulation parity. Goldens and
solver tolerances are unchanged. Validation results are recorded in
[VERIFICATION.md](VERIFICATION.md#passive-syntax-and-model-schema-verification).
