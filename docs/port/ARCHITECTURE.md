# Architecture of the Rust port

Current capabilities and numerical limits are recorded in
[DIFFSOL_FAER_IMPLEMENTATION.md](DIFFSOL_FAER_IMPLEMENTATION.md); the central
[TODO.md](../../TODO.md) tracks remaining work. Rust edition 2024 / MSRV **1.89**
applies to the locked workspace, not the historical dependency-free scaffold.

## Layering

Crates may only depend on crates **below** them. `spice-core` has no
dependencies at all; nothing depends on `spice-cli`.

Internal production dependencies (arrows mean “depends on”; external backends
are omitted):

```text
spice-cli      -> spice-analysis, spice-devices, spice-netlist, spice-core
spice-analysis -> spice-devices, spice-maths, spice-netlist, spice-core
spice-devices  -> spice-maths, spice-netlist, spice-core
spice-netlist  -> spice-core
spice-maths    -> spice-core
```

`spice-maths` does not depend on the netlist or device layers. `xtask` is a
separate development consumer of the analysis/device/netlist/core APIs. The CLI
currently inspects/parses decks; results are produced through library APIs and
examples, not a CLI simulation command.

| Crate | Responsibility | Mirrors |
| --- | --- | --- |
| `spice-core` | `Real`, `Complex`, SPICE numeric literals with scale factors, node table and ground aliasing, error type, analysis taxonomy | `src/include/ngspice/`, parts of `src/spicelib/parser/inpeval.c`, `src/frontend/inpcom.c` |
| `spice-netlist` | Deck loading (title line, `+` continuations, comments), tokenizer, card classification, AST, incremental parser | `src/frontend/inp.c`, `src/frontend/inpcom.c`, `src/spicelib/parser/inp*.c`; future `.param` work: `src/frontend/numparam/` |
| `spice-maths` | Dense/sparse/complex storage, petgraph row-coupling topology, faer LU, bounded diffsol BDF; trap/Gear coefficient/history APIs pending | `src/maths/dense/`, `src/maths/sparse/`, `src/maths/KLU/`, `src/maths/ni/` |
| `spice-devices` | `Device` trait, scalar R/C/L/V/I factories/stamps, branch and state-slot binding, trial-versus-accepted state (`StateHistory`/`TrialState`, `&self` stamping, atomic `Circuit::accept_point`), immutable linear operators, `Circuit`, petgraph incidence topology, top-level model resolver, bounded passive geometry/temperature recipes and diode input schemas; nonlinear arithmetic pending | `src/spicelib/devices/` |
| `spice-analysis` | Linear `.op`, single-source `.dc`, complex `.ac`, explicitly selected bounded BDF, plots and ASCII rawfiles | `src/spicelib/analysis/`, `src/frontend/rawfile.c` |
| `spice-cli` | Command-line entry point: `spice-rs <netlist>` | `src/frontend/main.c`, `src/ngspice.c` |
| `xtask` | Automation: C golden capture/drift checks, Rust-engine numerical verify, CI | — |

## Design rules

1. **The C tree is the specification.** Every non-trivial behaviour carries a
   doc comment naming the C file and function it must match. When the C code is
   ambiguous, the doc comment says so instead of guessing.
2. **Unimplemented means loud.** Stubs return
   `SpiceError::NotYetPorted { what, c_reference }`; `todo!()`/`unimplemented!()`
   are denied by clippy at the workspace level. `spice-cli` maps that error to
   exit status `3`, so scripts can distinguish "not ported yet" from a real
   failure.
3. **No FFI in the port.** The C library is only ever reached out-of-process, by
   `xtask` driving the `ngspice` binary to produce comparison data. A Rust
   `unsafe_code = "forbid"` workspace lint enforces this.
4. **Differentiable at the token level.** The tokenizer keeps the exact source
   text and column of every token, so parser errors can be reported against the
   original deck.
5. **Numbers are `f64` until proven otherwise.** ngspice mixes `double` and
   `float`; the port uses `Real = f64` and records any place where the C code
   loses precision in `float` as a documented divergence risk.

## Graph representations: prefer petgraph

Use petgraph 0.8.3 for graph storage and algorithms rather than maintaining
custom adjacency lists, DFS/BFS, union-find, SCC or topological-sort code. It is
already in the locked dependency graph; `spice-maths` and `spice-devices` now
use it directly in production APIs, not just dependency smoke tests.

- `Circuit::topology()` returns a petgraph `UnGraph` incidence snapshot. Each
  circuit node (including ground and unused nodes) and each device is a vertex;
  edges carry zero-based terminal ordinals. Parallel edges preserve repeated
  terminals and multiport devices without replacing them with terminal cliques.
  Device vertices retain insertion ordinals, preserving deck/branch-current
  order. `finalize()` validates this projection before rebuilding unknowns.
- `SparseMatrix::coupling_graph()` returns an `UnGraphMap` whose vertices are
  **matrix rows** and edges are assembled nonzero off-diagonal positions in
  either direction. Duplicate stamps are folded before extracting edges, so
  exact cancellation does not invent a coupling. Empty/diagonal-only rows stay
  present. Extraction leaves numeric storage untouched; rectangular matrices
  are rejected because rows/columns must describe the same unknown set.
- Use petgraph algorithms such as `connected_components`, `has_path_connecting`
  and its traversal types on these graphs. In M1c/M1d, use directed petgraph
  graphs for dependencies rather than a custom graph implementation. M1c now
  checks canonical file/section include cycles with `has_path_connecting` on a
  `DiGraph`; subcircuit elaboration and parameter evaluation graphs remain pending.

These graphs have different semantics. `NodeId::GROUND` is a **circuit** node;
MNA eliminates ground, so matrix row 0 is an ordinary unknown. Petgraph indices
are snapshot-local handles, not domain IDs or matrix numbering. A structural
path through a capacitor or multiport device does not prove a conductive DC
path. Disconnected matrix blocks may all be nonsingular, and connected matrices
may be singular. Do not reject circuits/blocks based on generic connectivity;
analysis-specific topology rules remain bounded; production LU performs
numerical rank/residual diagnostics. Graph construction does not validate
finite matrix values.

Snapshots rebuild on demand so mutations through `devices_mut()`, late nodes,
new stamps or matrix clearing cannot leave a stale internal adjacency cache.
Node-name interning, model-name sets, device-name lookup, ordered parameter
vectors and numeric matrix entries are **not graph algorithms**: keep their
purpose-built tables/order instead of forcing them into petgraph. `spice-core`
remains dependency-free and SPICE node IDs/ground aliasing remain unchanged.

## Winnow semantic parsing (`new-parsing`)

The semantic parser is implemented with winnow 1.0.4 (MIT), replacing M1a's
manual token cursor. Only `spice-netlist` directly depends on it; default
features are disabled and only `std`/`parser` enabled. Its declared MSRV is 1.65,
below this workspace's historical 1.85 requirement. The diffsol/faer integration
raises the workspace MSRV to 1.89 because the locked diffsol-la/nalgebra graph
requires it; minimum-version CI tests that graph. `Cargo.lock` pins the resolved graph;
offline builds require the registry dependencies to have been cached first.

- `parser/grammar.rs`: `Stateful<TokenSlice<Token>, Context>` over borrowed
  tokens. Read-only context carries the card, ground-alias configuration and
  model-declaration name index;
  `alt` dispatches end, analysis, model, device and explicit error branches.
- `parser/linear.rs`: tuples compose terminal/parameter grammars; `opt`, `peek`
  and `repeat` express optional scalars and repeated assignments. AST creation
  happens only after a complete card succeeds.
- `parser/syntax.rs`: shared positioned-name and finite-scalar primitives.
- `parser/model.rs` and `parser/diode.rs`: model cards with an optional outer
  parenthesis pair and bounded tail flags, and two-terminal diodes with scalar
  geometry/IC and OFF flags.
- `parser/transistor.rs`: three/four-terminal Q and four-terminal M forms;
  declared-model lookahead chooses port count. `flags.rs`, `ic.rs`, `waveform.rs`
  and `vector.rs` add positioned bare flags, bounded IC vectors and numeric
  PULSE/PWL in the same ordered assignment storage, not runtime semantics.
- `parser/structure.rs`: winnow subcircuit/X and source-directive grammars;
  formal/X values remain ordered and unevaluated.
- `parser/scopes.rs`: ordered body assembly and scope-local forward model-name
  indexing, including ancestors but excluding siblings/children/control scripts.
- `parser.rs::prepare_cards` and `parser/resolution.rs`: cache positioned cards
  and replay failures in order; resolved sources use bounded readers and a
  petgraph dependency graph. See [FRONTEND_STRUCTURE.md](FRONTEND_STRUCTURE.md).
- `cut_err` commits after a recognised card, device or parameter prefix. A required
  missing value, numeric overflow or an unported expression must never be
  mistaken for a missing optional/repeated element.
- A custom winnow error adapter preserves `SpiceError::Parse` source locations
  and `NotYetPorted` C references. `Parser::parse` enforces complete token
  consumption; `.end` deliberately consumes its tail to match C termination.

The initial backend rewrite preserved loader/tokenizer contracts. M1b's
model/D/Q/M slices and M1c's ordered scoped/source storage extend that AST;
`parse_deck` is syntax-only and `parse_file` now resolves sources. This is still
not completion of the remaining M1 syntax/round-trip gate.

## Two data models for a netlist

`spice-netlist` distinguishes:

- **`RawCard`** — a logical line plus its token stream and a coarse `CardKind`
  classification (`Device { designator }`, `DotCommand(..)`). Produced by the
  tokenizer; already implemented.
- **`Netlist`** — the semantic model (device instances with textual parameters,
  `.model` cards, `.subckt` bodies, analyses, includes). The incremental parser
  constructs a **bounded subset**: scalar and declared-model R/C/L instances,
  DC/AC/PULSE/PWL V/I sources, D/BJT/MOS/R/C/L model cards, bounded D/Q/M
  flags/IC instances, opaque analyses, nested subcircuits/X instances and
  source-relative includes/library selections. Ordered `ScopedCard` entries
  refer to typed vectors in the owning scope and retain raw source/provenance.
  Parameter evaluation, flattening and serialization are still unported.

Keeping both means the front-end can be ported incrementally: classification and
tokenization are useful on their own (the CLI can report what a deck contains
without needing semantic parsing), and the semantic model can evolve without
breaking the tokenizer's contract.

Parameter values retain their original numeric spelling. Positional values map
onto canonical instance parameters (`resistance`, `capacitance`, `inductance`,
`dc`); AC specifications become `acmag`/`acphase` with C's defaults. The parameter
vector records **application order**, not a dictionary: `INP2V()`/`INP2I()` apply
a leading DC value after named parameters; `INP2D()`/`INP2Q()` do the same for
a leading diode/BJT `area`. MOS accepts no unlabeled scalar. This order must
survive serialization and elaboration. A passive scalar before its model is
applied before named setters; one after its model is applied after them.
Analysis arguments remain unvalidated until their
consumer interprets them; AST success is not a promise of simulation support.

Model cards retain lowercased parameter names, original scalar text and bounded
bare family flags, without checking the scalar keyword schema or applying defaults. Their
`level` field is the first explicit raw scalar; selector/default/range rules
belong to the `spice-devices` model resolver, not this syntax layer.
D references can remain unresolved;
Q/M require a declaration in their body or an ancestor before `.end` so port/model roles are not
inferred from numbers or parameter keywords. Forward references work. The
first declared name wins over an optional substrate interpretation, as in
INP2Q. Q retains the three/four supplied ports; an omitted substrate's implicit
ground belongs to elaboration. M requires four ports and refuses a declared
model in the bulk slot. Model names are never ground-aliased. Bounded bare OFF,
model-family flags, Q/M IC vectors and PULSE/PWL now parse; arities, omissions,
C references and stricter delimiter policy are in [FRONTEND_VALUES.md](FRONTEND_VALUES.md).
Extra/thermal terminals, sensitivity flags, model binning, CIDER and numeric-looking
Q/M model names remain outside this grammar. Purely numeric Q model names produce Parse
errors: C's front end requires an alphabetic character; ngspice-47+ also rejects
the scaled-numeric `123n` probe. Ordinary alpha-named models containing digits
are covered by the live oracle.

## Deliberate divergences from the C code

Tracked here so they are never mistaken for bugs. Each also appears as a doc
comment at the divergence site.

| Divergence | Reason |
| --- | --- |
| `parse_spice_number_prefix()` consumes `MEG`/`MIL` in full, while `INPevaluate()` leaves those letters unconsumed | `INPevaluate()` returns a value and a rest pointer; consumers (`inpcom.c` scale scanning, `INPevaluateRKM_*`) do the skipping themselves. The Rust API reports bytes consumed, so it consumes the whole recognised suffix. The numeric result is identical. |
| RKM-style literals (`4k7` meaning `4.7k`, `inp2r.c`/`inp2c.c`/`inp2l.c`) are **not** accepted | Not ported yet; `parse_spice_number("4k7")` returns `None` and the semantic parser returns `NotYetPorted`. Tests pin this so the change is deliberate. |
| Scalar grammars reject non-finite numeric literals instead of passing them to a device | Input validation prevents overflow/NaN from entering the solver; errors retain token locations. |
| `gnd` aliasing is restricted to port positions, after model-name disambiguation | C's `inp_fix_gnd_name()` does a broader delimiter-based replacement in card text, including model identifiers. The AST deliberately preserves model/parameter spelling. Reserved-name collisions between `gnd` and `0` are not C-parity-proven. |
| Model parameter parentheses must form one optional balanced outer pair | The C tokenizer gobbles parentheses as delimiters. Diagnosing unmatched/nested pairs avoids accepting malformed scalar cards; comma delimiters are still accepted. |

The syntax subset requires an explicit scalar on model-less R/C/L instances.
Declared-model passives may omit it and retain bounded scalar geometry setters.
Numeric-looking passive model references after a scalar, quoted expressions and extended
flags remain explicit gaps (braced expressions parse unevaluated, see PARAM_EXPRESSIONS.md); numeric initial values remain scalars, even when a
model has the same name. This is not a claim about legality of wider C forms.

## Model resolution and schema boundary

`spice-devices::models::ModelResolver` indexes one deck's top-level declarations
case-insensitively, keeps the first definition and checks designator/family and
family-specific levels. `ResolvedModel` borrows the raw card; syntax never applies
model defaults. `spice-devices::schema::ScalarSchema` is an extensible ordered
finite-scalar validator with units, ranges, defaults and last-set provenance.
Initial typed diode inputs cover IS/N/RS/AREA/TEMP/TNOM. `ModelContext` passes
Celsius temperatures explicitly, without depending on `spice-analysis`.

`Circuit::add_instance` stages node/device changes and rejects missing models,
bad schema inputs and unavailable factories without changing existing numbering.
D/Q/M equations are still unavailable. Bounded model-backed R/C/L now delegate
effective values to existing scalar stamps; unsupported or unused model cards
cannot silently disappear from a successful simulation.
[MODEL_SCHEMAS.md](MODEL_SCHEMAS.md) records API examples, selector policy, C
references and deliberate bounded divergences. Diode APIs remain input validation,
not completed nonlinear setup or scoped expansion.

`passive::PassiveParameters` retains immutable validated recipes, not progressively
adjusted values. `AnalysisContext::model_context` copies Celsius TEMP/TNOM into the
device layer; all four drivers use `Circuit::linear_system_with_context`.
`LinearContext` carries that explicit context; `StampContext` carries circuit
and nominal temperatures too. `Circuit::from_netlist_with_context` validates at
nondefault temperatures without locking later runs to them. The model wrapper
computes effective R/C/L and delegates to existing stamps, preserving node/branch
namespaces, factor guards, DAE restrictions and accepted-state semantics.
[PASSIVE_MODELS.md](PASSIVE_MODELS.md) records formulas, units and supported forms.
