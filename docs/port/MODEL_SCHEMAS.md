# Passive syntax and initial model infrastructure (#11 / #17)

PR #51 merged these prerequisites (#11/#17). The initial infrastructure did
not itself implement model-backed passive arithmetic or D/Q/M simulation.
This checkout now adds #19's bounded passive arithmetic, pending merge; see
[PASSIVE_MODELS.md](PASSIVE_MODELS.md). D/Q/M simulation remains unavailable. The parser now accepts all eight rawfile fixture decks (#12/#13 scoped/source syntax);
`golden verify` still verifies three linear fixtures and excludes five.

## Bounded passive syntax

`parser/linear.rs` uses a scope-local declaration index, including ancestor and
forward declarations before `.end`, excluding child/sibling/control scopes.
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
Unknown passive names, expressions/flags, malformed/trailing tokens and overflowing
scalars never result in a partial successful AST. Model-less R/C/L still require
an explicit scalar. No AST parameter/default is synthesized for omitted values.

References: `inp2r.c`, `inp2c.c`, `inp2l.c` and ordered `INPdevSet` calls.
The Rust-only tree contains references, not copies of those C sources.

## Resolver and typed APIs

Public interfaces live in `devices`, not the parser or maths crates:

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
  rejects non-scalar kinds (including parsed flags/IC vectors), unknown names,
  nonliteral/nonfinite values and retains the last value
  plus its source location. Defaults have no setter location. `get` is
  case-insensitive. The original AST is never mutated.
- `ResolvedModel::parameters(&schema)` validates a device-owned model schema,
  excluding `level` already consumed by the resolver. Future passive/BJT/MOS
  owners add their own schemas/typed conversions; #19 adds tested passive recipes
  via `ResolvedModel::passive_parameters`. Owners must not claim physics
  from parser lookup or generic validation alone.

Example (also covered by module rustdoc and production-interface tests):

```rust
let resolver = devices::ModelResolver::new(&netlist.models)?;
let model = resolver.resolve(&netlist.devices[0])?.expect("diode reference");
let context = devices::ModelContext::default(); // TEMP/TNOM = 27 Celsius
let model_inputs = model.diode_parameters(&context)?;
let instance_inputs = model.diode_instance_parameters(&netlist.devices[0], &context)?;
// Both records are validated inputs, not an available simulation factory.
```

`ModelContext` avoids a lower-crate dependency on `analysis`. Callers may
explicitly pass circuit and nominal temperatures; `.option` cards are resolved by
`analysis::RunConfig` (see FRONTEND_STRUCTURE.md), whose context is passed here. All temperatures must be finite and exceed absolute zero.

## Raw levels, backend selection and applied setters

`ModelCard.level` remains the first **raw** scalar; ordered assignments retain all
occurrences. The resolver verifies that raw cache rather than rewriting it.
Integer conversion is `floor(raw + 0.5)`, following `inpgval.c`. Every level must
be finite, nonnegative and round into 0..=99; Rust rejects C warning/fallback or
unsafe-cast cases instead of silently changing the input.

| Family | Backend selection | Initial supported level policy |
| --- | --- | --- |
| BJT NPN/PNP | First explicit rounded level; default 1 | 0/1/2 (classic BJT); other selectors fail |
| MOS NMOS/PMOS | First explicit rounded level; default 1 | MOS1 (1) and MOS3 (3); other selectors fail naming the C directory `inpdomod.c` selects (`mos2/`, `mos6/`, `mos9/`, `bsim3/`, ...) |
| JFET NJF/PJF | First explicit rounded level; default 1 | 0/1 (JFET level 1), 2 (Parker-Skellern `jfet2/`), see [JFET.md](JFET.md); others fail with `NotYetPorted` |
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

MOS level schemas are owned by the level modules on the shared
`devices::mos` shell (M10, #89): `devices::mos1::MODEL` and
`devices::mos3::MODEL` are allowlists of C's `MOSxmParam` setters with
`mosXset.c` defaults; setters whose presence changes C's derivations
(`VTO`, `KP`, `GAMMA`, `PHI`, `NSUB`, `TPG`, `NSS`, `CJ`, `CJSW`, `CBD`, `CBS`,
`RD`, `RS`, `RSH`, `TNOM`, `U0`) have no schema default, aliases (`VT0`,
`UO`, MOS3 `DELVT0`) apply last-set-wins, and every other keyword is
rejected. MOS3 adds `XL`, `WD`, `XW`, `DELVTO`, `VMAX`, `XJ`, `NFS`, `ETA`,
`DELTA`, `THETA`, `KAPPA` (default 0.2), defaults `TOX` to 1e-7 m and `MJSW`
to 0.33, and has no `LAMBDA`; `XD`, `ALPHA` and `INPUT_DELTA`, listed in
`MOS3mPTable` but without a `MOS3mParam` case, are rejected.

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

## Model binning (#109)

C: `spicelib/parser/inpgmod.c::INPgetModBin` (with `in_range`/`parse_line`),
`misc/string.c::model_name_match`, `spicelib/parser/inp2m.c`,
`spicelib/parser/inpmkmod.c::INPmakeMod`, `frontend/subckt.c`. Rust:
`devices::binning` (selection rule), `ModelResolver::{resolve, select_bin,
bin_candidates, declarations_for, with_bin_options}`, the `M` grammar in
`netlist/parser/transistor.rs` and `devices::subckt` (scoped renaming).

The rules were read from the C sources and confirmed with the reference binary
(`show all : model` after `op`; `tests/c_binning_reference.rs`):

| Aspect | C behaviour (mirrored by the port) |
| --- | --- |
| Which instances | Only `M`. `inp2m.c` tries `INPgetMod` (exact name) first and calls `INPgetModBin` only when it fails, so an exact `.model nch` always wins over `nch.1`. Q/D/other designators never bin. |
| Candidate names | `<name>.<digits>`: a dot and at least one ASCII digit, nothing else (`nch.01` binds `nch`; `nch.a`, `nch.` and `nch1` do not). Case-insensitive, as the deck is lowercased. |
| Binnable models | `nmos`/`pmos`/`nsoi`/`psoi` whose level selects BSIM3 (8, 49, any version), BSIM4 (14, 54), HiSIM2 (68) or HiSIM-HV (73). Every other candidate (MOS level 1, 2, 3, 6, 9, ...) is skipped, so level-1 `.N` cards never bin. |
| Bounds | The candidate must set all four of `lmin lmax wmin wmax` (last setter wins, as `parse_line` overwrites); otherwise it is skipped. |
| Geometry | `l` and `w` must both be written on the instance; `defl`/`defw` and model defaults are not consulted. `L = l*scale`, `W = w/nf*scale`; `nf` divides only when the instance sets `nf` and either the instance's `wnflag` is nonzero or, without one, the `wnflag` option is set (default 0; 1 only under HSPICE/Spectre compatibility). The multiplier `m` is ignored. |
| Range test | `min < v < max` or `|v-min| < 1e-9` or `|v-max| < 1e-9`: both edges inclusive with an absolute 1 nm tolerance, so adjacent bins overlap on their shared edge (`l=5.0009u` is inside `lmax=5u`, `5.0011u` is not). |
| Multiple matches | Not an error. `INPmakeMod` prepends to the model table (keeping the first of duplicate names), and `INPgetModBin` returns the first match in that table: the **last declared** matching bin wins. |
| No match / missing `l`,`w` / no binnable candidate | The token is not a model; `inp2m.c` ends with "could not find a valid modelname". |
| Subcircuits | `subckt.c` rewrites an `M` model token to `<inst>:<name>` when any body-local model `model_name_match`es it (exact or bin). The innermost frame with such a match captures the name even if none of its bins fits the instance; outer bins or an outer exact model are then not considered. |

The port's resolver selects exactly that card. Differences are explicit:

- C's failure cases are `SpiceError::Parse` at the instance, naming every
  candidate with its location and the reason (no binnable family, missing
  `l`/`w`, or the effective L/W outside every bin).
- No binnable family is simulated yet. A selected BSIM/HiSIM bin reaches the
  ordinary level selector and fails with `NotYetPorted`
  (`m1 binned to model 'nch.2': ... selector 8 ...`), so deck-visible binning
  is gated per family by `levels()`; nothing is silently dropped.
- `.options scale` and `.options wnflag` are not deck settings in this port
  (`scale` is rejected as an unimplemented front-end option; `wnflag` is not an
  accepted option), and the `M` grammar does not accept `nf`/`wnflag`
  instance parameters yet. `devices::binning::BinOptions` and
  `ModelResolver::with_bin_options` carry both inputs, and the selection
  logic is unit-tested for them, so a later front-end change only has to
  supply the values.
- Subcircuit-local models are flattened to `<path>.<name>` (C: `<path>:<name>`).
  A body's bin set is emitted whole, in declaration order, and the `M`
  reference becomes `<path>.<name>`. A root declaration that would join or
  shadow such a set (`.model x1.nch.7` or `.model x1.nch`) is a parse error
  instead of silently changing the selection.
- The unused-root-model rule counts every candidate of a binned reference as
  used (C's `inp_rem_unused_models::mark_all_binned`).

To enable binning for a family (slice 10, BSIM3): make `levels()` accept the
selector and dispatch the factory on it. `ModelResolver::resolve` already
returns the selected bin's card; the factory reads its parameters as for any
other card, and size-dependent `L*`/`W*`/`P*` parameters are the family's own
schema concern.

## Circuit boundary and tests

`Circuit::add_instance` takes a resolver/context, validates before construction
and stages changes. On any error, existing node IDs, device ordinals and branch
rows remain unchanged. Successful scalar insertion still requires `finalize`
to rebuild numbering. `Circuit::from_netlist` uses this path with default context.
D/Q/M factories stay unavailable; valid diode inputs still end in `NotYetPorted`.
The bounded passive wrapper now computes contextual effective values and delegates
to existing R/C/L stamps, as documented in [PASSIVE_MODELS.md](PASSIVE_MODELS.md). Unused model declarations remain explicit unsupported
inputs instead of disappearing from a successful scalar simulation. Registry
ported flags are unchanged.

Ordinary tests: `tests/netlist_passive_models.rs` and
`tests/models.rs` cover syntax, scoping/order, raw AST preservation,
missing/wrong models, namespace collisions, rounding/cache/range failures,
defaults, provenance, invalid context and atomic failure. They need no C binary.

Opt-in probes (unique temporary directories, no FFI or golden writes):

```sh
NGSPICE_BIN=/absolute/path/to/ngspice cargo test -p ngspice-rs --test c_reference --locked -- --ignored
NGSPICE_BIN=/absolute/path/to/ngspice cargo test -p ngspice-rs --test model_c_reference --locked -- --ignored
```

`passive_models.cir` pins setter order and geometry-based C setup values.
`model_schemas.cir` pins first declarations, default/explicit diode IS/N/RS/AREA/
TEMP/TNOM, repeated diode integer levels and first BJT/MOS selector behavior.
These are setup/input checks, not Rust nonlinear simulation parity. Goldens and
solver tolerances are unchanged. Validation results are recorded in
[VERIFICATION.md](VERIFICATION.md#passive-syntax-and-model-schema-verification).

## URC line model (`urc`, #85)

`.model name urc(...)` (`urc.c` `URCmPTable`, defaults from `urcsetup.c`),
validated by `devices::urc` when a `U` instance is expanded:

| Setter | Unit | Domain | Default |
| --- | --- | --- | --- |
| `k` | — | positive, not 1 | 1.5 |
| `fmax` | Hz | finite | 1e9 |
| `rperl` | ohm/m | positive | 1000 |
| `cperl` | F/m | nonnegative (positive without `isperl`) | 1e-12 |
| `isperl` | A/m | positive | none: given selects the diode ladder |
| `rsperl` | ohm/m | nonnegative | 0 |

The bare `urc` flag is a no-op (`URC_MOD_URC`). `level` and unknown setters
are refused (C warns and ignores them). Instance setters `l` (m, required,
positive) and `n` (`IF_INTEGER`, rounded, at least 1). See [URC.md](URC.md).
