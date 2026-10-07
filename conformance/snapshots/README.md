# Token and AST snapshots

Deterministic text dumps that make parser changes reviewable (GitHub #21).
They are produced by `spice_netlist::dump` (a library module; nothing here
depends on the CLI). No C toolchain, sibling checkout or rawfile golden is
involved, and `conformance/golden/*.raw` is never touched.

## Layout

```
conformance/
  netlists/*.cir  parser/**/*.cir  cases/*.cir     inputs (recursive *.cir)
  snapshots/
    tokens/<input-path-without-.cir>.tokens        # token-dump v1
    ast/<input-path-without-.cir>.ast              # ast-dump v1
```

`cases/` holds focused ambiguity and error decks (numeric node/model names,
brace expressions, scoped names, continuation columns, options/globals,
waveforms/ICs, and positioned error diagnostics). Included files
(`parser/sources/{parts,shared}`) are covered by the AST dump of their root deck
(`via=[...]` include chains), not dumped separately. A malformed deck is
snapshotted as an `error` block, never as a partial AST.

## Schemas

Documented in the module docs of `crates/spice-netlist/src/dump.rs`. The first
line is a version header (`# ngspice-rs token-dump v1`, `# ngspice-rs ast-dump v1`).
Token spelling, kind, order and `line:column` (byte columns, relative to the
joined logical card) are preserved; AST scopes list ordered cards with
scope-local indexes, then typed vectors with ordered parameter assignments,
expression trees with spans, include chains, options, globals and params.

## Paths and platforms

Only the fixture root is normalized: paths print relative to `conformance/`
with `/` separators (`PathMapper`). Backslashes, drive-letter case and the
`\\?\` prefix are handled textually, so output is identical on Windows and
Unix (unit-tested with backslash inputs). Absolute paths outside the root print
as `<external>/<file name>`; OS error wording in diagnostics is replaced by
`<os error>`. `.gitattributes` forces LF for snapshots and inputs, so byte
comparison works on Windows checkouts. A literal `\` inside a Unix file name is
indistinguishable from a separator and is not supported in fixture names.

## Commands

| Command | Effect |
| --- | --- |
| `cargo test -p spice-netlist --test snapshots` | compares byte for byte; never writes; failure prints the first differing line and the bless hint |
| `cargo xtask snapshots` | dry run: lists created/changed/removed files, exits non-zero on drift |
| `cargo xtask snapshots --bless` | writes created/changed files, deletes orphans, reports each; a second run reports no changes |

## Schema-change procedure

1. Change the dump code and bump the version constant (`TOKEN_DUMP_HEADER` or
   `AST_DUMP_HEADER`) for any change to line shapes, spellings or ordering.
2. Run `cargo xtask snapshots --bless` and confirm a second run is clean.
3. Review the whole snapshot diff; in the PR, explain why the schema changed.
   Parser behavior changes must show up as intentional snapshot diffs, never as
   silently accepted ones. Do not mix this with rawfile golden recapture.
