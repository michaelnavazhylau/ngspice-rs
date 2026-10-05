# Verification

## The principle

The C tree is never linked. The port is compared against the reference
implementation **out of process**: `cargo xtask golden capture` drives the C
`ngspice` binary over a fixed set of decks and stores what it prints, and the
test suite reads those stored files. The workspace lint `unsafe_code = "forbid"`
exists to keep it that way — there is no FFI surface to widen by accident.

Two consequences worth stating plainly:

- `cargo test` needs neither the C tree nor a built `ngspice`. The goldens are
  committed data, so the suite runs anywhere.
- Drift is detected by re-running the reference implementation:
  `cargo xtask golden check`. Nothing in the port's own tests can detect that the
  C binary changed under it.

## Fixtures

`conformance/netlists/*.cir` are **pure decks**: no `.control` section, exactly
one analysis card, no file I/O. `conformance/netlists/README.md` explains why,
and the table there says what each fixture exercises.

## Capturing

`ngspice -b -r out.raw deck.cir` does not produce a usable rawfile here: with no
`set filetype` the output is binary, and `-r` is honoured before any control
script can change that. So `xtask` instruments each deck by inserting

```spice
.control
set filetype=ascii
run
write <fixture>.raw
.endc
```

immediately before the first `.end` card, runs `ngspice -b` on it with a scratch
directory (`target/xtask/golden/<fixture>/`) as the working directory, and reads
the ASCII rawfile that `write` produced. A fixture that already contains a
`.control` section is rejected rather than instrumented.

The scratch directory is also why `write` takes a bare file name: the path never
needs quoting, and `.include`-style relative paths would resolve the same way
every time (they are out of scope for fixtures anyway).

## Storage and comparison

Goldens are stored **verbatim**: `conformance/golden/<fixture>.raw` is the byte
sequence `write` produced, minus nothing. Faithfulness matters more than tidy
diffs, and it means a golden can always be traced back to a command.

Comparison ignores exactly one line, the `Date:` header, because it comes from
the clock. Everything else is compared, including `Command:`, which carries the
`ngspice-<version>` string — so a change of reference binary is reported as
drift rather than silently absorbed.

## Commands

| Command | Effect |
| --- | --- |
| `cargo xtask golden capture` | capture every fixture, write `conformance/golden/*.raw`, report new/changed/unchanged |
| `cargo xtask golden check` | capture into the scratch directory and report drift; writes nothing, exits non-zero on drift |
| `cargo xtask golden list` | describe each committed golden: plot name, variable count, point count, finiteness |
| `cargo xtask ci` | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` |

Locate the reference binary with `--ngspice <PATH>` or `NGSPICE_BIN`; otherwise
`build/src/ngspice` under the workspace root is tried (a convenience that only
exists inside a built ngspice checkout), then `ngspice` on `PATH`.
A relative `--ngspice` is resolved against the workspace root, not the caller's
directory, and stored as an absolute path so it still resolves inside the scratch
directory.

## What the tests check

`crates/spice-analysis/tests/golden_rawfiles.rs`:

| Test | Claim |
| --- | --- |
| `every_fixture_has_a_golden_and_no_golden_is_orphaned` | the two directories are in one-to-one correspondence |
| `every fixture is documented in EXPECTATIONS` | a new golden cannot be added without a human writing down what it contains |
| `fixtures_have_the_expected_shape_and_values` | plot name, flags, point count, variable names and hand-checked values |
| `goldens_are_well_formed_and_finite` | every plot has headers, variables with units, at least one point, and only finite values |
| `ngspice_lowercases_the_deck_title` | see the findings below |
| `the_writer_agrees_with_ngspice_apart_from_non_round_trippable_decimals` | the rawfile writer reproduces ngspice's layout line for line |
| `exactly_one_golden_token_is_not_round_trippable` | the one tolerated difference is bounded and named |
| `the_writer_reaches_a_fixed_point` | a second pass through the writer changes nothing |
| `the_transient_golden_matches_the_analytic_rc_curve` | the transient golden is physics, not just recorded numbers |
| `the_diode_golden_is_self_consistent` | sweep monotonicity, bounds, and Kirchhoff on the resistor |

The analytic and self-consistency tests matter as much as the value checks: a
golden captured from the wrong deck, or from a broken build, is internally
consistent but wrong. Checking it against a closed-form solution is what makes
the data trustworthy.

## Findings

Behaviours discovered while capturing, which the port has to reproduce. Each is
pinned by a test so that it cannot be lost.

1. **ngspice lowercases the deck title.** `Title: rc divider, operating point`
   for a deck whose first line is `RC divider, operating point`.
2. **The point index is not tab-separated.** `raw_write()` prints `" %d"`
   followed by `"\t%.*e\n"` for each value, and a blank line after every point.
   So the first value line of a point is `" 0\t<value>"` and the rest are
   `"\t<value>"`, with `""` between points.
3. **A complex plot has two spellings for a zero imaginary part.** A vector
   ngspice has flagged real is written `re,0.0`; one it has not is written
   `re,0.000000000000000e+00`. `.ac` flags nothing real, so even `frequency` and
   `v(in)` carry the long form — this is why `spice_analysis::Variable` has an
   `is_real` flag rather than inferring it from the data.
4. **`%.15e` is not always a round-trip.** A rawfile value carries 16
   significant digits, and two adjacent doubles can share one 16-digit spelling.
   Exactly one token in the current corpus shows this: `time` on line 19 of
   `rc_transient.raw` is written `1.000000000000000e-11`, but the nearest double
   to that decimal prints as `9.999999999999999e-12`. Re-serialising a golden is
   therefore not always byte-exact, which is why the writer is checked by
   numeric comparison plus a fixed-point property. This does **not** weaken the
   port's parity goal: when the Rust engine computes the same double ngspice
   computed, the same `%.15e` formatting produces the same bytes.
5. **`TEMP` and `TNOM` default to 27 °C.** `Doing analysis at TEMP = 27.000000
   and TNOM = 27.000000` on every run, matching
   `AnalysisContext::default()`.
6. **A DC sweep writes a blank line between points**, which the rawfile parser
   has to skip; a blank line is therefore not a reliable plot separator inside
   the values section.

## M1a parser verification

`crates/spice-netlist/tests/linear_parser.rs` checks AST fields against the
committed divider, AC low-pass and RLC decks: terminal order, values, source
locations, request arguments, case folding and ground aliasing. It also checks
that all five remaining fixture decks fail at an explicit unported construct,
not with a partially successful AST. `crates/spice-cli/tests/parse.rs` checks
process exits: supported parse = 0, missing file = 2, unported syntax = 3.

A separate opt-in oracle uses `conformance/parser/linear_sources.cir` to compare
parsed scalar parameters with C's `print @instance[parameter]` after an `.op`
setup. It runs out of process in a unique temporary directory and uses no FFI:

```sh
NGSPICE_BIN=/path/to/ngspice cargo test -p spice-netlist --test c_reference -- --ignored
```

This pins bare-AC defaults (magnitude 1, phase 0), implicit DC zero, source
leading-DC precedence, and R/C/L values and initial conditions. The last-set
parameter map from Rust is compared numerically with C instance queries. The
probe uses simple values; its tolerance is `1e-12` relative, with a `1e-12` scale
floor for zero, because C's `print` format is shorter than rawfile decimals.
The test is ignored by default so ordinary tests remain C-toolchain-independent.
It was run successfully against the local ngspice-47+ binary for M1a.

`conformance/parser/` is **not** part of the rawfile fixture corpus; do not add
`.raw` files there or confuse these instance-query checks with engine parity.
Token/AST snapshots and normalized-deck round trips are still M1d work.

## Winnow backend regressions

The existing M1a fixture, CLI and live C-oracle tests are retained unchanged.
`crates/spice-netlist/tests/winnow_parser.rs` adds checks that cuts preserve
terminal and missing-value diagnostics, optional slots cannot swallow overflow,
repetition cannot hide unported expressions, AC lookahead leaves following
keywords untouched, and trailing device tokens are never silently ignored.
Unicode-node diagnostics remain byte-column-based and separate parser calls
cannot leak backtracking state.

Cache the locked dependencies with `cargo fetch --locked` before an offline
check (`cargo test --workspace --locked --offline`). The zero-external-dependency
claim applies to the historical M0/M1a backend, not the `new-parsing` rewrite.

Worktree-only publication checks run via
`bash scripts/tests/publish-rust-only.sh`. They use disposable local repos and a
local bare remote to verify branch routing and dirty/mismatched-target refusal;
they never contact GitHub and are not part of `cargo xtask ci`.

## Not yet verified

Nothing about the port's own simulation arithmetic: there is no engine to compare yet.
`cargo xtask golden verify` — running the Rust engine over the same fixtures and
diffing against the goldens — is milestone work, not scaffold work. When it
lands, the comparison should be numeric with an explicit tolerance, and the
tolerance itself should be justified rather than guessed, because the solver's
iteration order will not match ngspice's.

The verification surface that does exist is honest about its limits: it proves
that the fixtures are real ngspice output, that they are reproducible on this
machine, and that the rawfile layer round-trips them. It proves nothing about
whether the port can simulate them.
