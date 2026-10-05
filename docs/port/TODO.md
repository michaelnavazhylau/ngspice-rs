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

## Next: M1b — Models and remaining fixture syntax

- [ ] Parse `.model` name/type/level and scalar parameter assignments (`inpdomod.c`).
- [ ] Parse D instances: two terminals and model (`inp2d.c`).
- [ ] Parse Q instances: collector/base/emitter, optional substrate, model (`inp2q.c`).
- [ ] Parse M instances: drain/gate/source/bulk, model and geometry (`inp2m.c`).
- [ ] Parse model-backed R/C/L without mistaking models for parameter references.
- [ ] Represent source waveforms, starting with `PULSE`, without implementing time evaluation.
- [ ] Pin malformed syntax and unsupported variants; enable diode/BJT/MOS/transient fixture AST tests.

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
Do not maintain two independent copies of the Rust files.
