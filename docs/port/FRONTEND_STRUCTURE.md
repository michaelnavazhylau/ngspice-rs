# Scoped cards and source resolution (#12 / #13)

This checkout parses all **8/8 rawfile fixture decks**, including
`subckt_divider`. This is syntax coverage, **not simulation or the full M1
round-trip gate**. Subcircuit flattening (#18) remains M5 work. The current
linear circuit builder explicitly rejects decks containing definitions/source
directives, and X factories remain unavailable.

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

`spice_analysis::RunConfig` (docs in `config.rs`) accepts `temp`, `tnom`,
`reltol`, `vntol`, `abstol`, `method`, `maxord`. Repeats override in order; a name
used both as flag and value, unknown names (including `no_auto_gnd`, a front-end
variable) and invalid values are errors; every other `cktsopt.c` option is
`NotYetPorted`. Tests: `spice-netlist/tests/options_globals.rs`,
`spice-analysis/tests/run_config.rs`, `spice-cli/tests/parse.rs`.

Next: **#15** parameter evaluation over the #14 expression AST; then
**#20–22** normalized serialization, deterministic snapshots and the complete
M1 eight-fixture round-trip gate. None of those gates is closed by 8/8 parsing.
