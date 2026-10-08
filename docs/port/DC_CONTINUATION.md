# DC continuation policy (#34)

Bounded, typed and reported continuation for nonlinear DC bias solves
(`.op`, `.ac` bias, and `.dc` points once the sweep runner reads the same
settings). Code: `crates/spice-analysis/src/bias.rs` (policy, report, solve),
`newton.rs` (iteration limit, counted failures), `config.rs` (deck options).
Tests: `crates/spice-analysis/tests/dc_continuation.rs`.

This is a demonstrated, bounded policy, **not** ngspice parity: see
[Deliberate differences from C](#deliberate-differences-from-c).

## Contract

1. **Direct first, preserved.** Linear circuits keep the exact one-shot solve
   (no continuation, no budget). A nonlinear circuit first runs plain Newton from
   the seed at full sources and zero artificial gmin; if that converges the result
   and iteration count are unchanged from before this issue.
2. **Only numerical failure retries.** Continuation starts only after a
   `SpiceError::Numerical` (iteration limit, singular or nonfinite system).
   Structural, device, unsupported or invalid-input errors are returned at once,
   unchanged, with no retry. A device that must fail loudly is never masked.
3. **Success solves the original equations.** Every strategy ends with one extra
   solve at **full sources and zero artificial nodal gmin**, with the same
   iterate *and* reloaded physical-residual tests as direct Newton. Only that
   solve is ever returned. A circuit that is singular without regularization
   (floating diode, parallel ideal voltage sources) therefore fails even though
   its gmin-regularized stages converge.
4. **Continuation is invisible to everything else.** Stages are disposable loads
   against the circuit's *empty* state history, so no trial/accepted device
   state is adopted, no accept hook runs and no stored source value is edited.
   Source scaling and overrides act on a temporary RHS only. The returned trial
   state belongs to the final solution, not to a previous iterate. Callers accept
   the point explicitly (`Circuit::accept_solution`/`accept_point`).
5. **Fixed junction gmin is distinct.** The device's own 1e-12 S junction gmin is
   part of the physical equations and is always present. "gmin" in the policy is
   only the *artificial* uniform nodal conductance added to non-branch rows.
6. **Deterministic, finite, bounded.** Schedules are explicit lists validated
   before the first load (finite, strictly progressing, size-capped). Work is
   capped per stage (`NewtonOptions::max_iterations`, 1..=10000) and in total
   (`ContinuationPolicy::max_total_iterations`; default
   `stages * max_iterations`, so never tighter than the per-stage limit implies).
   Failed stages are charged the iterations they actually used
   (`newton::solve_counted`). A stage cut short by the budget ends the solve as
   `BudgetExhausted`; it is never retried.

Order: direct Newton, then gmin stepping (seed start), then source stepping
(restarting from the all-sources-off state, as before this issue).

## API

```rust
use spice_analysis::bias::{ContinuationPolicy, DcSettings, SourceStepping, solve_dc_with};

let settings = DcSettings {
    newton: NewtonOptions { max_iterations: 50, ..Default::default() },
    continuation: ContinuationPolicy {
        gmin_schedule: vec![1e-3, 1e-6],               // empty disables
        source_stepping: Some(SourceStepping::uniform(10, 1e-8)?), // None disables
        max_total_iterations: Some(500),               // None = stages * maxiter
    },
};
match solve_dc_with(&circuit, &context, &settings, &[], Some(&seed), None) {
    Ok(solved) => { /* solved.solution.{values,trial,iterations}, solved.report */ }
    Err(failure) => { /* failure.error: SpiceError, failure.report: Box<DcReport> */ }
}
```

- `solve_dc` is unchanged in signature and behaviour: a compatible wrapper using
  `ContinuationPolicy::default()` that drops the report.
- `DcSettings::from_request(&AnalysisRequest)` resolves `rtol`, `vntol`,
  `abstol`, `maxiter`, `srcsteps`, `gminsteps`, `gminfactor` over the defaults.
  `NewtonOptions::from_request` now *rejects* the three continuation names
  (rather than ignoring them) so a Newton-only caller cannot drop them silently.
- `DcFailure` implements `Display`/`Error` and `From<DcFailure> for SpiceError`,
  so `?` works in `SpiceResult` code and keeps the actionable message.

### Defaults (unchanged behaviour)

| Setting | Default |
| --- | --- |
| gmin schedule (S) | `1e-3 ... 1e-12`, one decade per stage (10 stages) |
| source stepping | scales `k/20`, `k = 0..=20`, temporary gmin `1e-8` S |
| per-stage iterations | 200 |
| total budget | `stages * 200` (not binding) |

Limits: gmin schedule <= 100 stages, each in `(0, 1]` S and strictly decreasing;
source scales in `[0, 1]`, strictly increasing, <= 1001 scales, temporary gmin in
`[0, 1]` S; explicit total budget `1..=10_000_000`. Everything else is rejected
with a `SpiceError::Numerical` (context `DC settings`) before any load, reported
with `DcOutcome::Rejected`.

### Report

`DcReport` is returned on success (`DcSolution::report`) and failure
(`DcFailure::report`):

- `attempts`: strategies reached, in order, each `Converged`, `Failed` (with the
  failing scale/gmin and error) or `Disabled`.
- `stages`: every Newton solve with `source_scale`, artificial `gmin`,
  `iterations` and `error`; `is_unregularized()` marks full-source/zero-gmin stages.
- `total_iterations`, effective `budget`, and `outcome`: `Converged(strategy)`,
  `NonRetryableFailure`, `Exhausted`, `BudgetExhausted` or `Rejected`.
- `summary()` is embedded in the `Exhausted`/`BudgetExhausted` error text, e.g.
  `direct Newton: failed (...); gmin stepping: disabled; source stepping: ...`.

## Options and precedence

Highest first, resolved **per name**:

1. explicit request arguments `maxiter=`, `srcsteps=`, `gminsteps=`,
   `gminfactor=` (and `rtol=`, `vntol=`, `abstol=`);
2. the deck's `.option` values, **last occurrence wins in deck order** across
   cards (`RunConfig::dc()` exposes the resolved values; `applied()` keeps every
   occurrence);
3. the defaults above.

| Deck option | Request key | Meaning |
| --- | --- | --- |
| `itl1` | `maxiter` | Newton iterations per stage, 1..=10000 |
| `srcsteps` | `srcsteps` | `0` disables source stepping, else `N` equal increments (1..=1000) |
| `gminsteps` | `gminsteps` | `0` disables gmin stepping, else `N` stages (1..=100) from 1e-3 S |
| `gminfactor` | `gminfactor` | ratio between stages, finite, `1 < f <= 1e6`, default 10 |

`gminsteps`/`gminfactor` set without the other rebuild the default number/ratio;
the built schedule is validated as a whole (a ratio that underflows is an
error, reported at the offending deck option). With the default ratio 10 and at
most 10 stages the schedule is exactly the literal decade ladder.

Propagation is honest:

- `.op`, `.ac` (bias) and `.dc` requests receive the deck values through
  `RunConfig::request`, and all three drivers read them via
  `DcSettings::from_request`: the scalar sweep runner passes those settings into
  `solve_dc_with` for every nonlinear or operator-rebuilding point. A deck
  `srcsteps`/`gminsteps`/`gminfactor` therefore configures `.dc` exactly as it
  configures `.op`/`.ac` (see [DC_SWEEPS.md](DC_SWEEPS.md)); the names are still
  refused by `NewtonOptions::from_request` so no analysis can silently ignore one.
- `.tran` on the companion driver reads them for its nonlinear initial bias
  (C `dctran.c` calls `CKTop` with `CKTdcMaxIter` and the same stepping
  settings): `RunConfig::request` forwards `maxiter`/`srcsteps`/`gminsteps`/
  `gminfactor`, which the companion driver validates through
  `DcSettings::from_request` and passes to `solve_dc_with` together with its own
  Newton tolerances. Linear circuits and `uic` runs perform no bias Newton solve
  (C skips `CKTop` under `uic` as well). With `backend=diffsol` a deck DC option
  is `Unsupported`.
- `itl2` (#110) reaches `.dc` only, as `trcvmaxiter`: every point after the
  first first tries a plain warm-started Newton bounded by it (continuation
  disabled, C `dctrcurv.c` `NIiter(CKTdcTrcvMaxIter)`); a numerical failure falls
  back to the full `itl1` solve with continuation from the same seed. Unset, the
  sweep keeps the single full solve per point. C also bounds its dynamic
  gmin/source-stepping stages with `itl2`; this port's fixed ladders use `itl1`
  per stage.
- `itl6` is an alias of `srcsteps` (one setting, last occurrence wins).
- `gmin` is now the junction gmin (`AnalysisContext::gmin`, see
  [FRONTEND_STRUCTURE.md](FRONTEND_STRUCTURE.md#option-coverage-110)); it is not
  the artificial nodal continuation conductance, which still ends at zero.
  `gshunt`/`cshunt` remain explicit `NotYetPorted` options.

## Deliberate differences from C

The C reference is `cktop.c`/`niiter.c` and `inpdoopt.c`/`cktsopt.c`; the C tree is
read-only here and was **not** re-read for this change, so the statements about C
below are from the port's earlier analysis and should be verified before they
are cited as parity facts.

- **Fixed deterministic ladders.** ngspice's default strategies adapt the gmin/
  source step to Newton progress (dynamic gmin, Gillespie source stepping); the
  explicit step-count variants are fixed ladders. This port has only fixed
  ladders, so a given deck/seed always visits the same stages and the work bound
  is known up front. `0` means *disabled* here (not "use the dynamic default").
- **`srcsteps` = exactly N equal increments** (scales `k/N`) with a temporary
  1e-8 S nodal gmin, then an unregularized solve; `gminsteps`/`gminfactor` build
  a decade-style ladder from a fixed 1e-3 S start. They are not C's schedule.
- **`itl1` default is 200**, not C's 100, to keep this port's existing accuracy
  and damping (global 0.2 V step limit, not per-junction PN/FET limiting).
- **Success is always unregularized.** Each strategy must finish with full
  sources and zero artificial gmin; no regularized result is ever returned.
- **Non-numerical errors are never retried**, and a total iteration budget
  exists (C has none beyond the per-solve limits).
- **Junction `gmin`** (default 1e-12 S, `.option gmin`) is a device-model
  conductance carried by `ModelContext`, separate from the artificial
  continuation conductance; unlike C's dynamic gmin stepping, continuation never
  changes it.
- `.tran` reads these options only for the companion initial bias (see above);
  `itl2` bounds only `.dc` warm starts.

## Validation

See the `dc_continuation` test target (difficult diode fails bounded direct
Newton and is solved by configured source stepping against the analytic diode
equation; gmin rescue of a singular Jacobian; disabled/exhausted schedules and
budgets; invalid scales/schedules/budgets; impossible ideal-source loops; final
unregularized failure; report contents; source/history/accept-hook invariance on
success and failure; request > deck > default precedence and duplicate-setter
order; transient rejection; OP/AC dispatch; a device that reports unimplemented
physics is surfaced unchanged as one unregularized direct attempt rather than
being retried against regularized equations; AC deck `itl1`/`gminsteps`/`srcsteps`
take the same last-set-wins and request precedence as `.op`). Existing `m4_gate`,
`run_config` and M1/M3 tests are unchanged except that `run_config` no longer
lists `srcsteps` as a not-yet-ported option.
