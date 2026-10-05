# Roadmap

Milestones are cumulative. Each one must end with `cargo xtask ci` green and,
where the milestone is observable from a deck, with at least one golden fixture
proving parity with the C binary.

## M0 — Scaffold ✅ (current)

Cargo workspace, crate boundaries, error vocabulary, CI task, golden-data
harness.

Exit criteria met:

- `cargo build`, `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check` all pass
- `cargo xtask golden capture` drives the C `ngspice` binary and writes ASCII rawfiles
- every unimplemented entry point returns `SpiceError::NotYetPorted` with the C file it will be ported from
- no external crate dependencies

## M1 — Netlist front end

Parse a deck into `spice-netlist::Netlist`: title, device instances, `.model`,
`.subckt`/`.ends`, `.include`/`.lib`, `.param`/`.option`, analysis cards.

- hand-written recursive-descent parser over the existing tokenizer
- expression evaluator for `.param` (`src/spicelib/parser/ifeval.c`, `inppas*.c`)
- `gnd` → `0` aliasing applied at parse time, controlled by `no_auto_gnd`

Exit criteria: round-trip every deck in `conformance/netlists/` into the AST and
back to a normalised deck text; golden fixtures for tokens and AST dumps.

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
CIDER, the Tcl/numparam front ends, and the interactive command interpreter.
These are the parts of the C tree that make it 723k lines; the port aims for a
correct, fast core with a plugin-shaped device registry that can absorb models
later. See `MAPPING.md`.
