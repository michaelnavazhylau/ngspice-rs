# Roadmap

Milestones are cumulative. Each one must end with `cargo xtask ci` green and,
where the milestone is observable from a deck, with at least one golden fixture
proving parity with the C binary.

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

`rc_divider`, `rc_lowpass_ac`, and `rlc_series` now parse into real ASTs.
`rc_transient`, `diode_dc`, `bjt_ce`, `mos_inverter`, and `subckt_divider`
still return specific `NotYetPorted` errors. Parsing does not imply simulation:
all device implementations and analysis drivers remain stubbed.

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

### Remaining slices

1. **M1b:** `.model`, D/Q/M instance syntax, model-backed passives, source
   waveform syntax. No device arithmetic yet.
2. **M1c:** `.subckt`/`.ends`, X instances, `.include`/`.lib` structure and
   resolution, with explicit scope and recursion/error rules.
3. **M1d:** `.param` expressions, `.option`, `.global`, normalized-deck writer
   and token/AST goldens. Full subcircuit circuit elaboration is still M5.

Exit criteria (not met yet): round-trip every deck in
`conformance/netlists/` into the AST and back to a normalised deck text; golden
fixtures for tokens and AST dumps. Track concrete work in [`TODO.md`](TODO.md).

## M2 — Linear DC operating point

Resistors, independent V/I sources, and a real linear solver.

- `spice-maths::sparse` LU factorisation and `dense` Gaussian elimination
- MNA stamping for R, V, I, and shorted L
- `.op` driver: assemble, factor, solve, build a `Plot`

Exit criteria: `.op` on resistive networks matches the C binary to 1e-12
relative; the RC-divider golden already in the tree is the first check.

## M3 — Reactive elements and transient analysis

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
