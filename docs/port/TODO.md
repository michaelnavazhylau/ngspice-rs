# Port TODOs

Persistent implementation checklist. See [ROADMAP.md](ROADMAP.md) for milestone
exit criteria and [VERIFICATION.md](VERIFICATION.md) for evidence. A checked
syntax item does **not** mean its device or analysis can simulate yet.

## Completed

- [x] M0: dependency-free workspace, crate contracts, CLI, CI task, C golden harness.
- [x] M1a: semantic AST for scalar R/C/L and DC/AC V/I sources.
- [x] M1a: analysis-card requests, terminal canonicalization and `.end` termination.
- [x] M1a: three existing decks parse; all other fixtures fail at explicit gaps.
- [x] M1a: parser regressions, CLI process-exit tests and opt-in live C scalar oracle.
- [x] Correct guide references: expression grammar vs deck dispatch; numparam vs input passes.
- [x] `new-parsing`: replace the manual semantic cursor with winnow token-stream grammars.
- [x] `new-parsing`: add committed-error/lookahead/full-consumption regressions; preserve M1a behaviour.
- [x] Make publication branch-aware: feature branches cannot overwrite public main.

## Current: M1b — Models and remaining fixture syntax (partial)

- [x] Parse scalar `.model` name/type/level/assignments for D/BJT/MOS/R/C/L (`inpdomod.c`, `inpgmod.c`); retain the first raw level, defer selector/schema validation.
- [x] Parse two-terminal D instances/model/scalar geometry (`inp2d.c`); preserve leading-area precedence and `perim` alias.
- [x] Enable `diode_dc` AST/CLI tests; add model/diode regressions and an opt-in live C scalar oracle.
- [ ] Add required remaining model/diode forms (flags; explicit thermal/CIDER gaps remain outside initial engine scope).
- [ ] Resolve models and apply typed defaults/selector/range validation during elaboration before simulation.
- [x] Parse Q collector/base/emitter/optional substrate, declared model and scalar assignments; leading area applies last (`inp2q.c`).
- [x] Parse four-port M, declared model and scalar geometry/IC components; reject unlabeled values (`inp2m.c`).
- [x] Enable BJT/MOS AST and CLI fixture tests; pin forward/ambiguous model roles, error replay order and scope boundaries.
- [x] Add live Q/M scalar/terminal-binding oracle and numeric BJT model rejection checks.
- [ ] Support required instance flags and vector IC forms; keep extra terminals/binning/advanced backend scope explicit.
- [ ] Parse model-backed R/C/L without mistaking models for parameter references.
- [ ] Represent source waveforms, starting with `PULSE`, without implementing time evaluation.
- [ ] Pin remaining malformed syntax/unsupported variants; enable the transient fixture AST test.

## M1c — Subcircuit and file structure

- [ ] Parse `.subckt`/`.ends` scope, ports, formal parameters and X instances.
- [ ] Diagnose unmatched/mismatched terminators and duplicate definitions.
- [ ] Decide nested-scope representation before accepting nested subcircuits.
- [ ] Parse `.include` and `.lib` paths/sections, preserving quoted paths.
- [ ] Implement source-relative resolution with recursion/cycle limits and source-location provenance.
- [ ] Enable `subckt_divider` AST tests; do not pretend parsing is flattening.

## M1d — Parameter semantics and full front-end gate

- [ ] Implement a bounded `.param` expression grammar and evaluator from numparam behaviour.
- [ ] Define evaluation order, scope, units and undefined/cyclic-reference diagnostics.
- [ ] Parse `.option` and `.global`; implement required ground scope rules.
- [ ] Add normalized-deck serialization that preserves source parameter application order.
- [ ] Commit deterministic token/AST snapshots with a documented regeneration path.
- [ ] Round-trip **all eight** rawfile fixture decks through AST and normalized text.
- [ ] Keep `cargo xtask ci` green; mark full M1 complete only after all exit gates pass.

## Before M2

- [ ] Resolve solver licensing/implementation strategy; no translation from LGPL KLU without review.
- [ ] Implement dense pivoted elimination and sparse LU with singular/non-finite diagnostics.
- [ ] Implement R/V/I MNA stamping, plus shorted-L DC handling.
- [ ] Build circuits from the AST and implement the `.op` driver.
- [ ] Add `golden verify` for Rust-engine results, starting with `rc_divider` at justified tolerances.

## Deferred, not forgotten

- [ ] M3: C/L companion models, integration, `.tran`, complex `.ac`.
- [ ] M4: nonlinear models, Newton iteration, stepping, `.dc` sweeps.
- [ ] M5: subcircuit elaboration/scoping, measurements/output selection, binary rawfiles.
- [ ] Decide advanced MOS/BSIM-family scope before nonlinear-device expansion.

## Development and publication

Implement and test in `ngspice-rs` (the C-reference worktree). After review and
commit, regenerate the Rust-only public tree with
`scripts/publish-rust-only.sh`; use `--push` only when publication is requested.
`rust-port` publishes to `main`; feature branches keep their name. The standalone
target must be clean and checked out on that public branch. Branch-routing
checks run locally with `bash scripts/tests/publish-rust-only.sh` and do not
contact GitHub. Do not maintain two independent copies of the Rust files.
