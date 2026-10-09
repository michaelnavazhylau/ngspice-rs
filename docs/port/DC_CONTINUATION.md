# DC continuation and Newton limiting (#34, #106)

Bounded, typed and reported continuation for nonlinear DC bias solves (`.op`,
`.ac` bias, every `.dc` point and the companion `.tran` initial bias), and the
Newton step control underneath it. Code: `crates/spice-analysis/src/bias.rs`
(policy, report, solve), `newton.rs` (iteration limits, phases, step control),
`config.rs` (deck options), `crates/spice-devices/src/limiting.rs` (device
voltage limiting). Tests: `crates/spice-analysis/tests/dc_continuation.rs`,
`tests/convergence.rs`, and the `m7_conv_*` C goldens.

Since #106 the default is ngspice's own algorithm: per-device junction/FET
voltage limiting (`DEVpnjlim`, `DEVfetlim`, `DEVlimvds`) and `CKTop`'s
adaptive continuation (`dynamic_gmin`, `new_gmin`, `gillespie_src`, or
`spice3_gmin`/`spice3_src` for explicit step counts). The port's original
global 0.2 V damping and fixed ladders remain as opt-in fallbacks. This is
parity of the *algorithm* on demonstrated decks, not of every iterate:
[Remaining differences from C](#remaining-differences-from-c) lists what is not
ported.

## Contract

1. **Direct first.** Linear circuits keep the exact one-shot solve (no
   continuation, no budget). A nonlinear circuit first runs Newton from the seed
   at full sources and zero artificial gmin (C `NIiter(ckt, itl1)`), unless
   `.options noopiter` skips it (C `CKTnoOpIter`).
2. **Only numerical failure retries.** Continuation starts only after a
   `SpiceError::Numerical` (iteration limit, singular or nonfinite system).
   Structural, device, unsupported or invalid-input errors are returned at once,
   unchanged, with no retry.
3. **Success solves the original equations.** Every strategy ends with a solve
   at **full sources, zero artificial nodal gmin and the configured junction
   gmin**, with the same iterate *and* reloaded physical-residual tests as
   direct Newton. Only that solve is ever returned (`DcStage::is_unregularized`).
   A circuit that is singular without regularization therefore fails even though
   its regularized stages converge.
4. **A returned point is an exact evaluation.** With device limiting, a load in
   which any device limited a voltage (or the `MODEINITJCT` load) is marked
   nonconvergent (C `CKTnoncon`), so Newton can only stop on a load evaluated at
   the iterate itself; the residual checks are never weakened.
5. **Continuation is invisible to everything else.** Stages are disposable loads
   against the circuit's state history; no trial/accepted device state is
   adopted, no accept hook runs and no stored source value is edited. Callers
   accept the point explicitly (`Circuit::accept_solution`/`accept_point`).
6. **Finite and bounded.** Settings are validated before the first load. Work is
   capped per solve (`itl1`/`itl2`), per adaptive strategy (at most
   `MAX_ADAPTIVE_STAGES` = 1000 stages; C stops only at its step floors) and in
   total (`ContinuationPolicy::max_total_iterations`). Failed stages are charged
   the iterations they used. A stage cut short by the budget ends the solve as
   `BudgetExhausted`.

## Newton step control

`NewtonOptions::limiting` (`StepLimiting`, request key `limiting=device|global`):

- **`device` (default)**: C's policy. `niiter.c` damps nothing globally; each
  nonlinear device limits its own controlling voltages in its load through
  `spice_devices::limiting::Limiter`:
  - the first load of an operating point is C's `MODEINITJCT`: diodes at
    `tVcrit`, BJTs at `vbe = tVcrit`, `vbc = vsub = 0`, MOS1 at `vbs = -1`,
    `vgs = type * tVto`, `vds = 0`, independent of the seed;
  - later loads limit against the voltages the device stored in the previous
    load (C `CKTstate0`), or, in a predicted load (transient timepoint or
    warm-started `.dc` point), against the last accepted ones;
  - diode: `DEVpnjlim`, reflected about BV in breakdown (`dioload.c`); BJT:
    `DEVpnjlim` on `vbe`, `vbc` and `vsub` (`bjtload.c`); MOS1: `DEVfetlim` on
    `vgs` (or `vgd` in reverse mode) against the previous `von`, `DEVlimvds`,
    `DEVpnjlim` on the forward bulk junction (`mos1load.c`);
  - the device evaluates and linearizes at the limited voltages (the Newton
    equivalent current is shifted accordingly) and flags the load
    nonconvergent (contract item 4). Devices C never limits (behavioural
    sources, switches) take full steps, as in C.
- **`global`**: the port's earlier policy, the largest nodal step scaled down to
  0.2 V and devices loaded exactly at the iterate. Kept for comparison and for
  tests written against its iteration counts.

The default `itl1` is C's 100 (it was 200 before #106).

## Continuation schedules

`ContinuationPolicy::schedule` (request key `continuation=ngspice|ladder`):

### `ngspice` (default): `cktop.c`

`NgspiceStepping { gmin_steps, source_steps, gmin_factor, adapt_iterations }`,
from deck `gminsteps` (default 1), `srcsteps`/`itl6` (default 1), `gminfactor`
(default 10) and `itl2` as written (default 50):

| Setting | Strategy (C function) |
| --- | --- |
| `gminsteps=1` | `dynamic_gmin`: artificial diagonal gmin from `1e-2 / gminfactor` S, adapted after each stage (`iters <= itl2/4` grows the factor by `factor^1.5`, capped at `gminfactor`; `iters > 3 itl2/4` takes its square root, floor 1.00005; a failed stage retries from the last converged state with the fourth root, giving up below 1.00005) down to the junction gmin, then the original equations at `itl1`. If that fails, `new_gmin` ("true gmin stepping"): the same walk applied to the *junction* gmin of every device (slow-stage floor 3), then the original equations |
| `gminsteps=n>1` | `spice3_gmin`: `n + 1` stages of artificial gmin `gmin * gminfactor^n ... gmin`, then the original equations at `itl1` |
| `srcsteps=1` | `gillespie_src`: all sources off (if that fails, an eleven-stage artificial gmin ladder from `gmin * 1e10` at zero sources), then the source factor raised from 0 by 1e-3, times 1.5 after a quick stage, halved after a slow one, divided by 10 (capped at 0.01) after a failure that retries the last converged factor; gives up when the raise falls below 1e-7 |
| `srcsteps=n>1` | `spice3_src`: `n + 1` equal source factors `i/n` with no artificial gmin |
| `0` | disables that family |

Order: direct Newton, gmin stepping (`dynamic_gmin` then `new_gmin`, or
`spice3_gmin`), then source stepping, as `CKTop`. The adaptive strategies and
`gillespie_src` restart from the zero vector in `MODEINITJCT` (C zeroes
`CKTrhsOld`/`CKTstate0`); each later stage continues the previous converged
stage in `MODEINITFLOAT`. Stage solves are bounded by `itl2` (effective
`max(itl2, 100)`), the gmin strategies' closing solve by `itl1`; the raw `itl2`
steers the adaptation (`adaptiter`).

### `ladder`: the port's original fixed ladders

| Setting | Default |
| --- | --- |
| gmin schedule (S) | `1e-3 ... 1e-12`, one decade per stage (10 stages); `gminsteps`/`gminfactor` rebuild it from 1e-3 S |
| source stepping | scales `k/20`, `k = 0..=20`, temporary gmin `1e-8` S; `srcsteps=N` gives `N` equal increments |
| per-stage iterations | `itl1` (default 100) unless `stagemaxiter`/`itl2` is set |

Deterministic stage lists, kept for comparison; selecting `ladder` with
`adaptiter` is an error.

## API

```rust
use spice_analysis::bias::{ContinuationPolicy, DcSettings, NgspiceStepping, solve_dc_with};

let settings = DcSettings {
    newton: NewtonOptions::default(),                 // limiting: Device, itl1 = 100
    continuation: ContinuationPolicy::ngspice(NgspiceStepping {
        gmin_steps: 4,                                // spice3_gmin
        ..NgspiceStepping::default()
    }),
};
match solve_dc_with(&circuit, &context, &settings, &[], Some(&seed), None) {
    Ok(solved) => { /* solved.solution.{values,trial,iterations}, solved.report */ }
    Err(failure) => { /* failure.error: SpiceError, failure.report: Box<DcReport> */ }
}
```

- `ContinuationPolicy::default()` is the ngspice schedule with C's defaults;
  `ContinuationPolicy::ladder()` the fixed ladders; `disabled()` no continuation.
  `skip_direct` is `noopiter`.
- `DcSettings::from_request(&AnalysisRequest)` resolves `rtol`, `vntol`,
  `abstol`, `maxiter`, `limiting`, `continuation`, `srcsteps`, `gminsteps`,
  `gminfactor`, `stagemaxiter`, `adaptiter` and `noopiter` (`0`/`1`).
  `NewtonOptions::from_request` *rejects* the continuation names so a
  Newton-only caller cannot drop them silently.
- `DcFailure` implements `Display`/`Error` and `From<DcFailure> for SpiceError`.

### Report

`DcReport` is returned on success and failure:

- `attempts`: strategies reached, in order (`Direct`, `GminStepping`,
  `JunctionGminStepping` for `new_gmin`, `SourceStepping`), each `Converged`,
  `Failed` (with the failing scale/gmin and error) or `Disabled`.
- `stages`: every Newton solve with `source_scale`, artificial `gmin`,
  `junction_gmin` (set only by `new_gmin`), `iterations` and `error`.
- `total_iterations`, effective `budget`, and `outcome`: `Converged(strategy)`,
  `NonRetryableFailure`, `Exhausted`, `BudgetExhausted` or `Rejected`.
- `summary()` is embedded in the `Exhausted`/`BudgetExhausted` error text. An
  `Exhausted` error also states that ngspice's next fallback, the transient
  operating point (`src/spicelib/analysis/optran.c`), is not ported
  (`bias::OPTRAN_NOT_PORTED`).

## Options and precedence

Highest first, resolved **per name**: explicit request arguments, then the
deck's `.option` values (last occurrence wins), then the defaults.

| Deck option | Request key | Meaning |
| --- | --- | --- |
| `itl1` | `maxiter` | direct solve and the gmin strategies' closing solve; deck values forwarded as `max(itl1, 100)` (`niiter.c` floor); default 100 |
| `itl2` | `stagemaxiter`, `adaptiter`, `.dc` `trcvmaxiter` | every other continuation stage (`max(itl2, 100)`, forwarded when the deck sets `itl1` or `itl2`; unset, stages use `itl1`, which equals C's effective 100 by default); the raw value steers the adaptive strategies; on `.dc` it bounds the warm start of every point after the first |
| `srcsteps`/`itl6` | `srcsteps` | see the schedule tables (`0..=1000`) |
| `gminsteps` | `gminsteps` | see the schedule tables (`0..=100`); with `spice3_gmin` the ladder `gmin * factor^n` is validated against the deck's `.option gmin` |
| `gminfactor` | `gminfactor` | finite, `1 < f <= 1e6`, default 10 |
| `noopiter` (flag) | `noopiter=1` | skip direct Newton |
| — | `continuation` | `ngspice` (default) or `ladder`; port-only |
| — | `limiting` | `device` (default) or `global`; port-only |

Propagation: `.op`, `.ac` (bias), `.dc` and the companion `.tran` initial bias
receive the deck values through `RunConfig::request`; with `backend=diffsol` a
deck DC option is `Unsupported`. `.dc` warm-starts every point after the first
with a plain Newton solve bounded by `trcvmaxiter` (C `dctrcurv.c`
`NIiter(CKTdcTrcvMaxIter)`, effective 100 by default under the ngspice
schedule) in `MODEINITPRED`, and falls back to the full `CKTop` sequence on a
numerical failure; under `continuation=ladder` without `itl2` each point runs
the full solve as before. `gmin` is the junction gmin (`AnalysisContext::gmin`),
which `dynamic_gmin`/`new_gmin` approach and `spice3_gmin` starts from.

## Remaining differences from C

- **No `OPtran` fallback.** When every `CKTop` strategy fails, ngspice (whose
  `optran` defaults are enabled in `init.c`) runs a transient to find the
  operating point (`optran.c`). The port stops with an `Exhausted` error that
  says so.
- **No predictor or bypass.** C's `MODEINITPRED` extrapolates device voltages
  (`DEVpred`, `xfact` in `bjtload.c`) and `bypass` skips device evaluation; the
  port limits a predicted load against the last accepted voltages and always
  evaluates (`.options bypass=0` only).
- **FET limiting flags convergence.** C sets `CKTnoncon` only for `DEVpnjlim`
  steps and leaves FET limiting to `mos1conv.c`'s current test; the port flags
  every limited load so the returned point is an exact evaluation.
- **After a failed final gmin step** C still runs the closing `NIiter` from the
  failed iterate; the port does not retain failed iterates and ends the
  strategy. `spice3_gmin` starts from the seed, not from the failed direct
  attempt's last iterate.
- **Not ported**: `gshunt` (C's `dynamic_gmin` target `max(gmin, gshunt)` and
  gshunt diagonal), `.options oldlimit` (`CKTfixLimit`), the front-end
  `dyngmin` variable (skip `new_gmin`), BJT quasi-saturation limiting (the
  quasi-saturation model itself is rejected), `OFF`/`IC` instance start values
  (rejected per device). These are `NotYetPorted` where they are inputs.
- **Bounded work.** The adaptive strategies stop after
  `MAX_ADAPTIVE_STAGES`, and a total iteration budget exists; C has neither.

## Validation

- `tests/convergence.rs`: `DEVpnjlim`/`DEVfetlim`/`DEVlimvds` behaviour, an
  overdriven junction solved by direct Newton at an exact point, the
  `MODEINITJCT` load never converging, `noopiter`, the stage sequences of
  `dynamic_gmin`, `spice3_gmin`, `gillespie_src` and `spice3_src`, schedule-
  dependent latch states and the `optran.c` failure.
- `tests/dc_continuation.rs`: the #34 contract (only numerical failures retry,
  unregularized success, budgets, invalid settings, invariance of history,
  sources and accept hooks, precedence), mostly under `limiting=global` and
  explicit iteration limits, plus the ngspice request/deck resolution.
- C goldens `m7_conv_latch_op`, `m7_conv_latch_gillespie_op`,
  `m7_conv_latch_spice3_gmin_op`, `m7_conv_latch_spice3_src_op`,
  `m7_conv_latch_tran`, `m7_conv_bjt_schmitt` and `m7_conv_cmos_schmitt`
  (see [VERIFICATION.md](VERIFICATION.md#m7-convergence-parity-106)).
