# Scoped cards and source resolution (#12 / #13)

This checkout parses all **8/8 rawfile fixture decks**, including
`subckt_divider`; the M1 round-trip gate is closed separately by #22 (see the
end of this document). This document describes the parser's scoped AST and
source resolution, which is syntax coverage. The `X` instance factory is no
longer unavailable: since #18 the production entry points expand subcircuits
with hierarchical naming, scoped parameters/models and `.global`
([SUBCIRCUITS.md](SUBCIRCUITS.md)). The linear circuit builder still rejects
decks with unresolved `.include`/`.lib` directives.

C references: `src/frontend/subckt.c` (`doit`, X extraction/translation),
`src/frontend/inpcom.c` (`inp_readall`, library-section preprocessing and
subcircuit parameter preprocessing). The port retains structure instead of
performing C's elaboration/preprocessing rewrites.

## Ordered, scoped AST

`Netlist.cards` and `Subcircuit.cards` contain `ScopedCard` entries in source
order. `ScopedCardKind` indexes the owning scope's existing typed vectors:
devices, models, subcircuits, analyses and includes. Semantic data is not copied
into another AST. Each entry retains a positioned `RawCard` and the include
chain, outermost directive first. Root `.end` and body `.ends` remain explicit.
The opening `.subckt` is the parent scope's definition entry; its body belongs
to the definition. Nested definitions have their own vectors/order and closing
location. Selected libraries also retain their opening/closing positioned cards
in `IncludeDirective.selected_section`.

`.subckt name [ports...] [params:] key=value ...` and
`Xname [nodes...] target [params:] key=value ...` preserve terminal order,
case-fold identifiers and apply optional ground aliasing only to terminals.
The last name before the parameter tail is an X target, stored in `model` with
designator `x`; parsing does not resolve the target, check port counts, detect
X recursion or instantiate anything. Zero-port helper definitions/instances
are retained. Duplicate subcircuit names in the **same** scope are errors;
shadowing in child scopes is allowed. Missing, unmatched and mismatched `.ends`
are errors, as is `.end` inside an open definition. Nesting is capped at 64.

Formal and X assignments are ordered, including duplicates. Values are a
single finite numeric token (`ParameterKind::Scalar`), a braced expression or
bare non-function name parsed into `ParameterKind::Expression` (#14; syntax
checked, not evaluated, see [PARAM_EXPRESSIONS.md](PARAM_EXPRESSIONS.md)), or
another single token such as a quoted value (`ParameterKind::Textual`). Original
spelling is retained; no defaults or parameter evaluation occurs.
Use braces/quotes for multi-token text. Missing values, overflow, malformed
braces and trailing input fail; `params:` requires an assignment. Scalar
consumers reject the expression and textual kinds.

Forward model-name disambiguation is local to each body, with ancestor names
visible and child/sibling names excluded. It is still only a name index, not
scoped model validation or elaboration. Analysis cards in bodies are retained
there, not promoted to top-level requests. Unsupported directives in any body
remain explicit errors.

## Syntax-only and resolved APIs

- `Parser::parse_deck(&Deck)` is syntax-only, with **no filesystem I/O**. It
  retains `.include path`/`.inc path` and `.lib path section` as unresolved
  directives. `path` is decoded and `path_spelling` retains quotes/escapes;
  `resolved_path`/`selected_section` are absent.
- `Parser::parse_file(path)` resolves sources before scoped assembly using
  default `SourceLimits`. `parse_file_with_limits` accepts explicit budgets.
  `spice-rs parse` uses this resolved API; summary/cards/tokens still inspect the
  original deck without source expansion.
- Only the root file has a title. Included sources are **fragments**, including
  their first physical line. Existing comment/continuation rules and joined-card
  byte columns remain unchanged.
- Paths resolve relative to the **containing source file**, not the process
  working directory. Resolved fragments carry canonical paths and original
  physical line numbers. A directive stays in its insertion scope and its
  expanded content follows it in order. Repeated/diamond includes remain
  repeated; no global deduplication suppresses content.
- `.lib path section` selects a case-insensitive `.lib section` …
  `.endl [section]` block. Section boundaries are validated across the source:
  missing, unmatched/mismatched, duplicate or nested section definitions fail.
  Ordinary non-library cards in unselected sections are neither tokenized nor
  resolved. Library directives are tokenized to distinguish references from
  boundaries. A two-argument `.lib path section` inside a selected section resolves normally.
  Bare library section markers are only supported in selected source libraries,
  not as ordinary semantic cards in a root deck or plain include.
- The first expanded `.end` stops the entire deck, including parent source
  processing. Cards/paths after it are not tokenized/resolved. An earlier
  semantic/lexical failure still wins over a later source-resolution failure;
  no partial AST is returned.

## Bounded dependencies and deliberate limits

A directed **petgraph 0.8.3** dependency graph uses canonical `(file, section)`
identities; incremental `has_path_connecting` checks reject reachable cycles,
including self-includes, symlink aliases and recursive library selections.
Distinct sections of the same file are separate identities. This adds a direct
use of the existing workspace MIT/Apache-2.0 dependency, not a new package or
version; MSRV remains 1.89. Subcircuit-reference and parameter graphs are future
elaboration/evaluation work, not implemented by name indexing.

Default limits per parse:

| Limit | Default |
| --- | ---: |
| Include nesting below root | 64 |
| File reads, including root/repeated sources | 1,024 |
| Aggregate bytes, including unselected sections | 16 MiB |
| Processed expanded cards, including directives/terminators | 100,000 |

Depth settings above 64 are rejected to protect the call stack. Bytes are read
with a bounded reader **before** constructing fragments; byte/card/file limits
also bound repeated acyclic work. UTF-8 input is required. These are explicit
port safety policies, not C parity claims. No environment/home/search-path
expansion, compatibility aliases, nested library sections, parameter evaluation,
source serialization or sandboxing is promised. Canonical paths follow symlinks;
files must remain stable during a parse. Include access is not restricted to the
root directory.

## Verification and next gates

`spice-netlist/tests/subcircuits.rs` and `sources.rs` exercise production AST/file
APIs: order/scope/forward names, nested definitions, textual duplicates,
terminators, paths/section selection, provenance, symlinks/cycles, repeated work,
limits and first-error/termination rules. The committed multi-file probe is
`conformance/parser/sources/main.cir`; CLI tests resolve it and accept all eight
rawfile fixtures. Device tests retain explicit unsupported/atomic failures.
An opt-in `c_reference.rs` test copies this source tree into a scratch directory
and compares the parsed selected model's RSH with live C setup; it is not a Rust
flattening or simulation comparison. No rawfile goldens/tolerances are changed.

## Options and globals (#16)

`.option`/`.options`/`.opt` parse (winnow, `parser/options.rs`) into
`OptionCard { settings: Vec<OptionSetting>, location }`; each setting has a
lowercased name, optional positioned value text (`None` for a flag) and its
location. Order and duplicates are kept; syntax only. `.global` parses into
`GlobalCard { nodes: Vec<GlobalNode> }` using the device-node normalization
(`gnd` -> `0` only with automatic aliasing). Cards are indexed by
`ScopedCardKind::Options(i)`/`Global(i)` into `Netlist::options`/`globals`.
`Netlist::is_global_node`/`global_node_names` are the flattener contract: `0` is
always global, other names only if a top-level `.global` listed them. Options or
globals inside a `.subckt` body return `NotYetPorted`.

`spice_analysis::RunConfig` (docs in `config.rs`) resolves the settings. Repeats
override in order; a name used both as flag and value, unknown names (including
`no_auto_gnd`, a front-end variable) and invalid values are errors. Every
accepted name has real semantics or is a documented no-op; the remaining
`cktsopt.c` names and front-end variables with an effect are `NotYetPorted`.
Tests: `spice-netlist/tests/options_globals.rs`,
`spice-analysis/tests/{run_config,options_coverage,dc_continuation}.rs`,
`spice-cli/tests/parse.rs`; opt-in C comparisons in
`spice-analysis/tests/c_options_reference.rs`; goldens `options_gmin_dc`,
`options_xmu_tran`.

### Option coverage (#110)

C references: `cktsopt.c` (`OPTtbl`, `CKTsetOpt`), `inpdoopt.c`,
`cktntask.c` (defaults), `frontend/spiceif.c::if_option`, `frontend/options.c`.

| Option | Status | Port semantics (request key) | C behaviour / justification |
| --- | --- | --- | --- |
| `temp`, `tnom` | effect | circuit / nominal temperature (C) | `TSKtemp`/`TSKnomTemp` |
| `gmin` | effect | `AnalysisContext::gmin` = `ModelContext::gmin`: parallel junction conductance of diode, BJT (B-E, B-C and the default substrate junction: collector for NPN/vertical, base for PNP/lateral; scaled by the instance `m`, not by `area`) and MOS1 (B-D, B-S) in every analysis; finite, `>= 0`, default 1e-12. It does **not** move the artificial DC gmin-stepping ladder (C's `spice3_gmin` starts at `gmin * gminfactor^gminsteps` and `dynamic_gmin` stops at `max(gmin, gshunt)`; this port's ladder starts at 1e-3 S and ends with a zero-artificial-gmin solve, see DC_CONTINUATION.md) | `CKTgmin` in `dioload.c`, `bjtload.c` (`m *` every conductance), `mos1load.c` |
| `reltol`, `vntol`, `abstol` | effect | `rtol`/`vntol`/`abstol` for `.op`/`.dc`/`.ac` Newton and `.tran` | `TSKreltol`/`TSKvoltTol`/`TSKabstol` |
| `chgtol`, `trtol` | effect | companion truncation control; rejected with `backend=diffsol` | `CKTterr` |
| `method`, `maxord` | effect | companion trap/Gear-2, orders 1..2 (3..6 `Unsupported`); rejected with diffsol | `TSKintegrateMethod`/`TSKmaxOrder` |
| `xmu` | effect | companion trapezoidal weighting `0..=0.5` (`xmu`); rejected with diffsol | `CKTxmu` in `nicomcof.c` (C accepts any value; > 0.5 is an explicit error here) |
| `itl1` | effect | Newton limit of the direct DC solve (`maxiter` = `max(itl1, 100)`) for `.op`/`.dc`/`.ac` and the companion `.tran` initial bias | `CKTdcMaxIter` in `cktop.c` (`dcop.c`, `acan.c`, `dctrcurv.c`, `dctran.c` call `CKTop`); `NIiter()` raises any limit below 100 to 100 (`niiter.c`). Unset, this port keeps 200 (C: 100), see DC_CONTINUATION.md |
| `itl2` | effect | Newton limit of every gmin/source-stepping stage of that DC bias (`stagemaxiter` = `max(itl2, 100)`, or 100 when only `itl1` is set) in `.op`/`.dc`/`.ac`/`.tran`; on `.dc` also the warm-started Newton limit at every point after the first (`trcvmaxiter`), falling back to the full bias | `CKTdcTrcvMaxIter` in `cktop.c` stepping stages and `dctrcurv.c` (same `niiter.c` floor; C default 50, effectively 100). **Divergence:** C's `dynamic_gmin`/`gillespie_src` also adapt their step from the raw `itl2/4`; this port's fixed ladders have no step adaptation (DC_CONTINUATION.md) |
| `itl4` | effect | companion Newton iterations per timepoint (`tranmaxiter` = `max(itl4, 100)`; unset, 100); rejected with diffsol | `CKTtranMaxIter` in `dctran.c`: nominal default 10, but `niiter.c` raises it (and any value below 100) to 100 |
| `srcsteps`, `itl6` | effect | source-stepping increments (`srcsteps`; `itl6` is the same setting) | `OPT_SRCSTEPS` (both names) |
| `gminsteps`, `gminfactor` | effect | gmin-stepping ladder | see DC_CONTINUATION.md |
| `acct`, `noacct`, `list`, `nomod`, `nopage`, `node`, `opts`, `noinit`, `norefvalue` | no-op (flag only) | none | front-end print controls handled first by `if_option`; no numerical effect; this port prints no such listing |
| `itl3`, `itl5`, `cptime`, `limtim`, `limpts`, `lvlcod`, `lvltim` | no-op (value validated) | none | `OPTtbl` entries without `IF_SET`; `if_option` warns "unsupported"/"obsolete" and C ignores them |
| `post`, `ingold` | no-op | none | plain front-end variables that nothing in ngspice reads |
| `indverbosity` | no-op (non-negative integer validated) | none | selects only which stderr diagnostics `muttemp.c` prints for an inductive system (`CKTindverbosity`); the port prints none and always rejects a coupled inductance matrix that is not positive semidefinite (MUTUAL_INDUCTANCE.md) |
| `bypass=0` | no-op | none | C default (`TSKbypass = 0`); this port never bypasses device evaluation. Other values `NotYetPorted` |
| `pivtol`, `pivrel` | `NotYetPorted` | | Sparse 1.3 pivot thresholds (`TSKpivotAbsTol`/`TSKpivotRelTol`, `spfactor.c`); this port's faer partial-pivoting LU has no equivalent knob yet |
| `gshunt`, `cshunt`, `rshunt`, `noopiter`, `oldlimit`, `numdgt`, `minbreak`, `defm`/`defl`/`defw`/`defad`/`defas`, `badmos3`, `trytocompact`, `keepopinfo`, `copynodesets`, `nodedamping`, `linesearch`, `absdv`, `reldv`, `noopac`, `epsmin`, `sparse`, `klu`, `klu_memgrow_factor`, `lte*`, `newtrunc`, XSPICE options | `NotYetPorted` | | |
| `filetype`, `savecurrents`, `scale`, `scalm`, `seed`, `seedinfo`, `rndseed`, `interp`, `warn`, `measureprec`, `rawfileprec`, `strict_errorhandling` | `NotYetPorted` | | front-end variables with an output/setup effect |
| anything else | parse error | | C would store an unread variable or warn |

No-ops are recorded with their reason in `RunConfig::ignored()` (`IgnoredOption`)
and named by `spice-rs parse` ("options without effect"); `applied()` lists only
settings with an effect. With `backend=diffsol`, any deck `itl1`/`itl2`/
`srcsteps`/`itl6`/`gminsteps`/`gminfactor`/`itl4`/`xmu` is `Unsupported`.

Iteration limits follow C's *effective* values: `NIiter()` (`niiter.c`) raises
every `maxIter` below 100 to 100, so `itl1`/`itl2`/`itl4` take integers in
`0..=10000` and are stored and forwarded as `max(n, 100)`; values below 100
change nothing in C (verified: identical results and iteration counts) or here.
The request keys `maxiter=`/`stagemaxiter=`/`trcvmaxiter=`/`tranmaxiter=` stay literal port
knobs (`1..=10000`) for tests and diagnostics. C's `IF_INTEGER` options round a
real value (`floor(x + 0.5)`); this port rejects a non-integer `itl*` value
instead (a safe, documented difference).

### Expression option values (#107, option part)

`{expr}` and single-quoted `'expr'` values parse (numparam grammar,
`parser/options.rs`) into `OptionSetting::expression` while `value.text` keeps
the written form; double-quoted strings and quotes containing escapes are parse
errors. `RunConfig::from_netlist` evaluates them against the deck's top-level
`.param` scope (one root scope, so later `.param` cards are visible), then applies
the usual range checks to the number; `RunConfig::from_options` has no scope and
returns `Unsupported`; `method` takes a word, never an expression. The writer
emits the value text verbatim and `semantic_eq` compares the parsed expression;
AST dumps add the expression tree. C quirk (not reproduced): ngspice aborts the
deck when a print flag such as `noacct` shares an `.options` card with a `{}`
value; this port accepts that card.

## `.ic`, `.nodeset` and `uic` (#27)

`.ic`/`.nodeset` parse (winnow, `parser/hints.rs`) into
`NodeHintCard { entries: Vec<NodeHint> }`; each `NodeHint` has the canonical node,
byte locations of the entry/node/value, and a `NodeHintValue` (finite literal with
original spelling, or an unevaluated `{expr}` that `elaborate::literalize`
evaluates against `.param`). Cards are `ScopedCardKind::InitialCondition(i)` /
`Nodeset(i)` into `Netlist::initial_conditions`/`nodesets`; entries keep card
order, entry order and duplicates (no dedupe; precedence is the consumer's).
Accessors: `Netlist::initial_conditions()`/`nodesets()` (entries, possibly
unevaluated) and `ElaboratedNetlist::initial_conditions()`/`nodesets()`
(`ResolvedNodeHint { node, value, location }`, finite). The `.tran` `uic` word is
removed from `AnalysisCard::arguments` and stored in `uic`/`uic_location`
(`AnalysisRequest::uic`); `Netlist::transient_uic()` finds it.

C (`inppas3.c`): accepts `V(name)` with an optional `=`; `I(..)`, `V(a,b)`, bare
names and non-numeric values give `.ic syntax error` / a netlist error. C silently
accepts ground nodes, a missing value, an empty card, non-finite values, and warns
and ignores unknown nodes. The port rejects the silent cases explicitly (ground,
missing node/value, empty card, non-finite, `V(a,b)`, `I(..)`) and reports
`.nodeset all=value` and any `.ic`/`.nodeset` inside a `.subckt` body (C
translates them in `subckt.c`) as `NotYetPorted`. Unknown nodes cannot be seen
without a circuit; the analysis layer rejects them with the entry's location (C only
warns "IC on non-existent node ... ignored"). Bare parameter names are not
references; use `{expr}`. The analysis half (landed): `RunConfig::from_netlist`
evaluates the entries against `.param` (`elaborate::literalize_node_hints`) and
attaches them, in deck order, to every `AnalysisRequest` as
`initial_conditions`/`nodesets` (`NodeCondition { node, value, location }`); the
last duplicate wins as in C. Semantics are in [TRANSIENT.md](TRANSIENT.md). AST dumps print
`initial-conditions`/`nodesets` sections and the `uic @loc` line only when present, so existing snapshots are unchanged. Tests:
`spice-netlist/tests/ic_nodeset_parser.rs`, opt-in `c_ic_nodeset.rs`,
`spice-analysis/tests/run_config.rs`, `initial_conditions.rs`.

## Normalized deck writer (#20)

`spice_netlist::write_netlist(&Netlist) -> SpiceResult<String>` serializes the
**raw, unevaluated, unflattened** AST. It is separate from debug dumps
(`spice-rs parse`), from evaluated/expanded decks (#15 `elaborate::literalize`, #18 flattening) and from byte-exact
source reproduction. The full contract is the module documentation of
`crates/spice-netlist/src/writer.rs`; in short:

- One card per line (no continuations/comments/blank lines), title first, 2-space
  indentation per `.subckt` level. `Netlist::cards`/`Subcircuit::cards` fix the
  card order, so forward model references, directive order and scope structure
  are unchanged; nothing is reordered or hoisted. `.end` is written only if the
  deck had one.
- Parameter lists keep application order and duplicates. Positional values are
  written by name where order matters (`rpost a 0 rm resistance=5k
  resistance=4k`; V/I leading DC becomes a trailing `dc`; D/Q leading area a
  trailing `area=`), an omitted Q substrate stays omitted, PULSE/PWL omissions
  stay omitted, `ic` vectors are `ic=(a,b)`. Numeric spellings are never
  re-formatted. Expression text is preserved, so grouping is exactly the
  source's; the writer verifies that each expression text re-parses to the
  stored tree and each `.param` card re-parses to the same assignments.
- **Includes:** the writer emits `.include`/`.lib` directives and skips every
  card with a non-empty `include_chain` (resolved content). Inlining would
  flatten source structure, invalidate source-relative paths and duplicate
  library files; there is no inline mode. The directive path keeps its original
  spelling when it still decodes to `path` (so quoted paths with spaces survive),
  else it is bare or double-quoted with `\\`/`\"` escapes; paths are never rewritten, so
  the written deck must live where its relative includes resolve.
- **Errors:** `SpiceError::Unsupported` for anything that would not re-parse to
  the same semantics (unknown designators/parameter names or kinds, gaps in
  PULSE fields, expression text/tree mismatch, inconsistent card indexes, names
  that are not single tokens or would be read as comments, scopes opened and
  closed in different files).
- **Reparse:** SourceLocs, joined card text and spans always differ. Use
  `semantic_eq`/`semantic_diff`/`semantic_form` (module `semantic`), which
  neutralise locations, `Netlist::path`, raw card text, `path_spelling` and the
  original text of waveform/`ic` vectors (their structured values are compared).
  Re-parse with the same `Parser` configuration (`auto_gnd`). Tests:
  `crates/spice-netlist/tests/deck_writer.rs` (all `conformance/netlists/*.cir`,
  `conformance/parser/*.cir` and the source-resolution fixture round-trip and
  reach a writer fixed point). The #22 gate (`m1_gate.rs`) builds on it.

Done: **#15** top-level parameter evaluation over the #14 expression AST
(`eval`, `elaborate`). **#21** adds `dump`/`snapshot` (versioned token/AST dumps, snapshots in
`conformance/snapshots/`, `cargo xtask snapshots --bless`). **#22** closes the
M1 eight-fixture round-trip gate in `crates/spice-netlist/tests/m1_gate.rs`
(per-deck counts, terminal/model roles and analyses; semantic round trip and writer
fixed point; snapshot match; combined fixtures `conformance/parser/combined_*.cir`;
negative cases). Scoped-name enforcement is structural: declarations stay in
their scope, and Q-family lookup sees only the local scope and ancestors (a
model from a sibling/child/other scope is a parse error). Unresolved `X` targets
and D/M model names parse unresolved; resolution belongs to elaboration
(#17/#18). Subcircuit flattening and subcircuit-scoped parameter evaluation are
implemented (#18, [SUBCIRCUITS.md](SUBCIRCUITS.md)); a D/Q/M parse is still
syntax, not simulation.
