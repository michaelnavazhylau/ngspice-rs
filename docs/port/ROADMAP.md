# Roadmap

Milestones group bounded deliverables; issue-level dependencies, not an entire
previous milestone, determine what can start in parallel. The historical M0/M1
completion notes below describe those stages, not current solver availability.
[TODO.md](../../TODO.md) is the only detailed implementation checklist;
[DIFFSOL_FAER_IMPLEMENTATION.md](DIFFSOL_FAER_IMPLEMENTATION.md) records current
production limits. Each deliverable must retain green workspace validation and,
where observable from a deck, production comparisons against C data.

## M0 — Scaffold ✅

Cargo workspace, crate boundaries, error vocabulary, CI task, golden-data
harness.

Exit criteria met:

- `cargo build`, `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check` all pass
- `cargo xtask golden capture` drives the C `ngspice` binary and writes ASCII rawfiles
- every unimplemented entry point returns `SpiceError::NotYetPorted` with the C file it will be ported from
- no external crate dependencies at M0 completion (the later `new-parsing`
  branch adds winnow deliberately)

## M1 — Netlist front end (current, in progress)

Parse a deck into `spice-netlist::Netlist`: title, device instances, `.model`,
`.subckt`/`.ends`, `.include`/`.lib`, `.param`/`.option`, analysis cards.

- winnow card-parser combinators over the existing tokenizer on `new-parsing`;
  expression combinators when parameter evaluation lands
- `.param` evaluator behaviour from `src/frontend/numparam/spicenum.c` and
  `xpressn.c`, with preprocessing in `src/frontend/inpcom.c`
- `gnd` → `0` aliasing applied at parse time, controlled by `no_auto_gnd`

**Guide correction:** `src/frontend/parse-bison.y` is an expression grammar,
not a deck grammar. `inppas*.c` are circuit input passes (models, devices,
initial conditions, shunts), not a `.param` evaluator. `ifeval.c` evaluates
behavioural-device expression trees; it is not the specification for `.param`.

### M1a — Linear syntax ✅

- scalar R/C/L values and named scalar instance parameters
- V/I leading or explicit DC and AC magnitude/phase, including AC defaults
- title, terminal order, numeric spelling and source locations preserved
- lowercased identifiers; ground aliasing restricted to terminal positions
- analysis cards retained as requests with **unvalidated** arguments
- `.end` stops parsing; other directives and device grammars fail explicitly

At M1a completion, `rc_divider`, `rc_lowpass_ac`, and `rlc_series` parsed into
real ASTs and five fixtures returned specific `NotYetPorted` errors. M1b's
model/diode slice below additionally unlocks `diode_dc`. Parsing does not imply
simulation: device implementations and analysis drivers were stubbed at that
stage. Main now includes the bounded linear engine described below.

M1a checks: parser fixture/unit regressions, CLI exit-contract tests, and an
opt-in C oracle comparing scalar AST parameters with live C instance queries.
See `VERIFICATION.md`. Token/AST golden dumps and serialization are **not yet
implemented** and remain part of the full M1 exit gate.

### `new-parsing` — Winnow backend ✅

Reimplemented the M1a semantic parser with winnow 1.0.4, using borrowed token
streams, card alternatives, optional/repeated parameters and committed errors.
The deck loader and tokenizer are unchanged; all M1a behaviour is retained.
Combinator regressions cover backtracking, required values, overflow, trailing
tokens and byte-based source positions. No remaining M1 syntax is unlocked by
this rewrite. See `ARCHITECTURE.md` for the dependency justification.

### M1b — Models and nonlinear instance syntax (partial)

Implemented bounded model/D/Q/M slices:

- scalar `.model` cards for D/BJT/MOS/R/C/L families, with lowercased identifiers,
  ordered textual assignments and the first explicit raw `level` value
- optional outer parentheses and C-style comma/whitespace assignment delimiters
- two-terminal D instances, model references, leading area and named scalar
  geometry/IC/temperature parameters; `perim` maps to `pj`
- C's leading-area precedence after named assignments, with opt-in model/diode
  scalar queries and fixture/CLI regressions

- three/four-terminal Q instances, first-declared-model disambiguation and
  leading area applied after scalar assignments; omitted substrate stays omitted
- four-terminal M instances with ordered scalar geometry and IC components;
  no unlabeled MOS values, binning, vector ICs or extra/thermal ports
- read-only top-level declaration indexing before `.end` for Q/M forward
  references, without leaking names across scopes or parser calls

`diode_dc`, `bjt_ce` and `mos_inverter` now parse, bringing supported fixture decks
to six. Q/M need declared names for arity. The current checkout additionally
retains declared-model R/C/L forms and omitted values (#11), including forward
references and pre-/post-model scalar precedence. Device-owned top-level
resolution, family/level checks and bounded diode inputs are implemented (#17);
see [MODEL_SCHEMAS.md](MODEL_SCHEMAS.md). PR #51 merged these prerequisites.
This checkout adds bounded model-backed passive arithmetic (#19, pending merge);
see [PASSIVE_MODELS.md](PASSIVE_MODELS.md). No nonlinear arithmetic is implemented. Flags, IC vectors,
thermal/CIDER forms, extended passive forms and waveforms remain pending.

### Remaining slices

1. **M1b:** required remaining model/device forms (flags, IC vectors),
   extended passive forms and source waveform syntax. No nonlinear arithmetic yet.
2. **M1c:** `.subckt`/`.ends`, X instances, `.include`/`.lib` structure and
   resolution, with explicit scope and recursion/error rules.
3. **M1d:** `.param` expressions, `.option`, `.global`, normalized-deck writer
   and token/AST goldens. Full subcircuit circuit elaboration is still M5.

Exit criteria (not met yet): round-trip every deck in
`conformance/netlists/` into the AST and back to a normalised deck text; golden
fixtures for tokens and AST dumps. Track concrete work in the central
[`TODO.md`](../../TODO.md).

## M2 follow-up — Linear-core verification and documentation

The original linear-core goal is implemented: scalar R/C/L/V/I elaboration,
faer dense/sparse real/complex LU, `.op`, independent-source `.dc` and complex
`.ac`. Production DC comparisons retain **1e-12 relative + 1e-15 absolute**;
AC retains **1e-10 + 1e-12**. M2 is follow-up, not a solver reimplementation.

The GitHub milestone groups Rust-engine golden verification (#7), bounded
model-backed passive elaboration (#19) and historical documentation correction
(#49). `cargo xtask golden verify` covers three committed linear fixtures and
explicitly reports the five excluded fixtures; see
[VERIFICATION.md](VERIFICATION.md#rust-engine-golden-verification).
PR #51 merged #11/#17, and their acceptance suite passes. This checkout
implements #19's bounded passive arithmetic and C/production checks, pending
merge. The M2 merge gate is not complete merely because this branch passes.

Exit gate: all three issue slices merged with production/failure-path tests,
justified relative and near-zero absolute bounds and accurate capability docs.
Preserve finite/rank/residual checks and explicit unsupported cases.

## M3 — Reactive elements and transient analysis

Main includes complex AC and a separately selected, bounded
adaptive BDF API. This does **not** complete M3 or change its trap/Gear-2 goal:
companions, `.ic`/`.nodeset`, waveform parsing and general MNA DAEs remain pending.

C, L, and the numerical integration machinery.

- trapezoidal and Gear-2 integration (`src/maths/ni/`)
- companion models for C and L, `.ic`, `.nodeset`
- `.tran` driver with adaptive timestep and truncation-error control
- `.ac` small-signal analysis over the complex MNA system

Exit criteria: RC and RLC golden fixtures for `.tran` and `.ac`.

## M4 — Nonlinear devices

Diodes and MOS level 1, with Newton–Raphson.

- `.dc` sweep, source stepping, `gmin` stepping, `.option` convergence limits
- diode, MOS1, and BJT models
- `.model` parameter handling with typed parameters and range checks

Exit criteria: diode rectifier, MOS inverter and BJT bias fixtures match C.

## M5 — Usability

- `.measure` (or a deliberate substitute), `.print`/`.save`/`.four`
- binary rawfile support, so unmodified C goldens can be consumed
- subcircuit instantiation semantics: `.global`, parameter passing, scoping

## Deliberately out of scope (initially)

BSIM-family and other advanced device models, XSPICE code models, OSDI/Verilog-A,
CIDER, the Tcl integration, the full numparam compatibility surface, and the
interactive command interpreter. Basic `.param` evaluation is still required
by M1; excluding the full numparam front end does not exclude that subset.
These are the parts of the C tree that make it 723k lines; the port aims for a
correct, fast core with a plugin-shaped device registry that can absorb models
later. See `MAPPING.md`.
