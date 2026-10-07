# Subcircuit instantiation: port binding, scoped parameters and models (#18)

**Implemented: top-level `.subckt` definitions, `X` instances, formal defaults,
instance overrides, `.global` nodes and body-local models.** `X` elaboration
runs inside the production entry points — `Circuit::from_netlist`,
`Circuit::from_netlist_with_context` and `spice_analysis::RunConfig::circuit` —
so a deck never reaches a device factory with an unexpanded instance.

C references (read-only): `src/frontend/subckt.c` (`inp_subcktexpand()`,
`translate()`, `translate_node_name()`, `translate_inst_name()`, `gettrans()`,
`settrans()`, `collect_global_nodes()`) and `src/frontend/numparam/spicenum.c`
for the parameter environment. The Rust side is
`crates/spice-devices/src/subckt.rs` (expansion),
`crates/spice-netlist/src/eval.rs` (`ParamScope::resolve_instance`) and the
model resolver in `crates/spice-devices/src/models.rs`.

## Supported subset

| Input | Behaviour |
| --- | --- |
| `.subckt name t1 t2 … [p=v …]` … `.ends [name]` | definition, name case-insensitive |
| `Xname n1 n2 … subckt [p=v …]` | instance; the last connection is the target |
| Formal default `p=v` or `p={expr}` | one `.param` definition, ordered before the body's `.param` cards |
| Instance override `p=v` / `p={expr}` | bound value in the instance scope, evaluated at the call site |
| Body `.param` | body scope, with the caller's scope as parent |
| Body-local `.model` | renamed per instance, visible only inside that instance and its descendants |
| Root `.model` referenced from a body | inherited through the parent scope, name unchanged |
| `.global n …` (root card) | `n` is never prefixed; ground `0` always is |
| Nested `X` inside a body | expanded recursively, path extended |

Connection order is binding order: `Xname a b sub` binds `sub`'s first terminal
to `a` and its second to `b`. Instance parameters are **named only**; the parser
treats a bare token in the parameter tail as another connection, which then
fails the arity check rather than silently binding positionally.

## Hierarchical naming

Ground and `.global` nodes keep their name. Everything else inside an instance
is renamed by prefixing the dotted instance path:

| Declaration | Expanded name |
| --- | --- |
| instance `x1` of `div`, node `mid` | `x1.mid` |
| instance `xi` of `inner` inside instance `xout` of `outer`, node `c` | `xout.xi.c` |
| resistor `r1` inside `x1` | `r.x1.r1` |
| source `v1` inside `xin` | `v.xin.v1` |
| `X` instance `xi` inside `xout` | `xout.xi` (no repeated designator) |

This is exactly C's rule, re-probed against the C binary with an internal node
and a nested call: node names `xout.mid`/`xout.x1.c`, device names
`r.xout.r2`/`r.xout.x1.r1`. Two instances of one definition therefore never share
a node, a device or a branch current, whatever their internal values are.
Because every device name is unique, `Circuit::finalize`'s duplicate-name and
dangling-terminal checks keep working unchanged on the expanded deck.

## Parameter precedence

Highest first:

1. **instance override** — `x1 in out div rval=3k`, evaluated at the call site in
   the caller's scope;
2. **body `.param`** — `rval=9k` inside the definition;
3. **formal default** — `rval=2k` on the `.subckt` line, evaluated in the
   instance scope, so it may reference a body `.param` and, through the parent
   link, the caller's scope.

Duplicates are applied in written order, so the last one wins:
`x1 in out div rval=1k rval=8k` uses `8k`. An instance parameter that the
definition does not declare is bound but inert — the same as C, which ignores it.

Precedence (2) over (3) follows from ordering: the defaults are handed to
`ParamScope::resolve_instance` as a single `.param` card **before** the body's
own cards. Precedence (1) over (2) needs the different binding rule of
`resolve_instance`: a body card that redefines an overridden name is kept in
`ParamScope::entries` as `ParamState::Superseded` and never evaluated, instead
of the explicit error `ParamScope::resolve` reports for an ordinary scope.

Why a formal default sees a body `.param` at all is C's numparam behaviour:
formals and `.param` cards share one environment and are ordered by dependency
level, not by card position (probed: a default `{base}` resolves against a body
`.param base=4k`, and a body `.param r2v={rval*2}` resolves against the formal
`rval`).

## `.global`, ground and the caller's names

- Ground `0` is global: a body node named `0` stays ground.
- A root `.global n` card makes `n` global for every body. `.global` inside a
  subcircuit body is rejected by the parser (see `parser/scopes.rs`).
- The exemption is decided from the canonical name, so with automatic gnd
  aliasing `.global gnd` is ground, while under `no_auto_gnd` `gnd` is an
  ordinary global node.
- A formal terminal bound to a global node keeps the global name; the binding is
  not applied. This matches C's `gettrans()`, which tests the global list before
  the formal table.

## Models

Lookup walks the frame chain, innermost first, so a body-local `.model` shadows
a root declaration of the same name inside that instance only; the root
declaration is unaffected elsewhere. Each instance's local declarations are
renamed to `<path>.<name>` (`x1.am`, `x1.xi.am`) and appear in the flattened
model list in first-reference order, after the root cards. Unused local
declarations are not emitted; an unresolved reference keeps its written name and
is reported by `ModelResolver` with the instance location. This is deliberately
asymmetric with the root scope, where an unused declaration is an explicit
`Unsupported` error: an unused local declaration cannot reach any device, so no
physics can be silently discarded, while a root declaration is visible to the
whole deck.

A rename may not collide with another flattened model. `<path>.<name>` is a
legal deck spelling (`.model x1.am r(...)` is a usable name), and the resolver
keeps the first declaration for a name, so a collision would silently solve the
device against the wrong card; `expand_subcircuits` therefore reports
`duplicate flattened model name '<name>'` instead. Root-to-root duplicates keep
their pre-existing first-declaration behaviour, because only the generated
rename is rejected.

Resolved declarations go through the same device-owned schemas as a flat deck
(`docs/port/MODEL_SCHEMAS.md`); expansion never picks a backend or validates a
family, so an unsupported level or family is still an explicit error from the
factory.

## Bounds, recursion and atomicity

- **Recursion** is rejected when an instance can reach it: `expand_subcircuits`
  builds the directed definition graph (an edge for every `X` target inside a
  body), takes the definitions reachable from a top-level `X`, and reports the
  first strongly connected component among those as
  `circular subcircuit definition: 'first' -> 'second' -> 'first'`, located at
  the definition's `.subckt` card. A cyclic definition nothing instantiates is
  dead text, exactly like an unused non-cyclic definition.
- `SubcircuitLimits { max_depth: 32, max_devices: 250_000 }` is the backstop
  against a legal but exploding deck, in addition to the parameter evaluator's
  own ceiling on evaluated nodes (`EvalLimits::max_nodes`), which is shared by
  the whole expansion because every instance scope is evaluated against one
  `EvalBudget`.
- Every failure — unknown subcircuit, arity mismatch, limit, unsupported body,
  parameter evaluation, model resolution, factory rejection — happens before the
  circuit is published. `expand_subcircuits` takes `&Netlist` and returns fresh
  vectors, and `Circuit::add_instances` builds every device against one staged
  node table and commits nodes and devices together, so a caller's node table,
  device list and branch rows are untouched by a failed deck.

## Diagnostics

Errors carry the original card location: a bad body device points inside the
definition, a bad `X` card or arity mismatch points at the invocation, and the
flattened device name (`r.x2.r1`) appears in model-resolution messages.

## Explicit exclusions

| Excluded | Reported as |
| --- | --- |
| nested `.subckt` inside a body | `NotYetPorted` at the definition |
| `.include`/`.lib` inside a body | `NotYetPorted` |
| analysis card inside a body | `NotYetPorted` |
| `.option`, `.global`, `.ic`, `.nodeset` inside a body | `NotYetPorted` (`parser/scopes.rs`) |
| `.include`/`.lib` at the root | `Unsupported` from the circuit entry points |
| textual/flag/formals without a finite default | `NotYetPorted`/`Parse` |
| `.subckt` ports given positionally | not SPICE syntax; the extra token is a connection and fails the arity check |
| `no_auto_gnd` through the circuit entry points | pre-existing: `Circuit` builds its node table with `gnd` aliasing always on, so a deck parsed with `Parser::with_auto_gnd(false)` still folds `gnd` to ground once a circuit is built. Expansion itself honours the parser's canonical names (see `gnd_is_ground_with_aliasing_and_only_global_under_no_auto_gnd`). |

A root `.model` that nothing references is still an error
(`unused model declarations`), including when a body shadows it: the
declaration is reported, never dropped silently.

## Validation

- `cargo xtask golden verify` runs `subckt_divider` through the production `.op`
  path and matches the committed C golden (`conformance/golden/subckt_divider.raw`)
  by variable name; the fixture is no longer in the excluded list.
- `crates/spice-analysis/tests/golden_rawfiles.rs` solves the same deck through
  `Circuit::from_netlist`.
- `crates/spice-devices/tests/subcircuits.rs` covers port binding, hierarchical
  identity, nesting, overrides/defaults, body parameters, globals/ground,
  local-model shadowing and sibling isolation, expansion limits, purity,
  batch atomicity and every diagnostic above.
- `crates/spice-netlist/tests/param_eval.rs` pins `resolve_instance`'s
  override-beats-body-card rule.
