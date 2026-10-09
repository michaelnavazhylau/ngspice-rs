# M4 — bounded nonlinear devices and convergence

Implemented in the Rust-only `work/m4-nonlinear` worktree, based on `9f0f6d1`
(the integrated M1 options and M3 trial/accepted-state/companion contracts).
This is a demonstrated **subset**, not full SPICE parity. There is no due date.
Publication targets the Rust-only repository; remote issue closure is separate
from this bounded local gate and its review.

## Workspace and ownership

Local workspace: `worktrees/ngspice-rs-m4/`. Its ignored `target` symlink shares
`../ngspice-rs-m3/target`; `c-reference` points to `../../ngspice_test`. Cargo's
existing global dependency cache is reused. These links are local conveniences,
not repository dependencies. Source, decks and goldens are independent worktree
files: symlinking editable files between branches would violate isolation.

- `devices::models` / `schema`: existing first-declaration family/level
  resolution, ordered last-set scalar projection, units, provenance and domains.
- `devices::nonlinear`: diode factory, junction equations and nonlinear
  charge integration. `transistors`: Ebers-Moll BJT and MOS1 factories/equations.
- `Device::assemble_small_signal` / `Circuit::small_signal_system`: conductance
  and charge Jacobians at an explicit bias. This is **not** immutable BDF assembly.
- `analysis::newton`: disposable load/solve/reload, physical iterate and
  equation-residual convergence, row equilibration and bounded voltage damping.
- `analysis::bias`: direct DC solve, nodal-gmin and source continuation.
- `analysis::sweep`: typed V/I/R/TEMP axes and bounded nested DC.
- Companion transient consumes all `Device::truncation_slots` charge/derivative
  pairs. State remains owned by the existing M3 history, not the model object.

The raw-card registry has no model-card context and deliberately still rejects
D/Q/M there. The working model-aware factory is `Circuit::from_netlist` /
`RunConfig::circuit` / `Circuit::add_instance`; raw-card coverage is not a promise
of a context-free nonlinear factory.

## Supported physics and explicit boundaries

Every unknown setter, including a parsed but unimplemented flag/IC vector, fails
before node interning. Scalar schema validation checks **all occurrences**, not
just the last one; original ASTs remain unchanged. Defaults follow the named C
setup routines; the code's schema tables are the exact allowlists.

### Diode

Model: `IS` (1e-14 A), `N` (1), `RS` (0 ohm), `TNOM` (context), `CJO` (0 F),
`VJ` (1 V), `M` (0.5), `FC` (0.5), `TT` (0 s). Instance: `AREA` (1), `M` (1),
`TEMP` (context). Grading and FC require `0 <= value < 1`; scale products and
all derived values must remain finite.

- Exponential forward law and C's cubic reverse continuation below `-3 N Vt`.
- A fixed 1e-12 S **junction** gmin, separate from artificial nodal continuation.
- RS creates an internal anode, with conductance `AREA*M/RS`.
- Depletion charge and capacitance, with a continuous quadratic continuation
  above `FC*VJ`; diffusion charge `TT * I_junction` and its actual derivative.
- Contextual saturation-current temperature scaling at fixed C defaults
  EG=1.11 eV, XTI=3. Non-nominal depletion-charge temperature laws are rejected
  when CJO > 0. No cumulative temperature adjustments.

References: `dio/dioload.c`, `diosetup.c`, `diotemp.c`.
Unsupported: breakdown/recovery, sidewall/tunneling/recombination/self-heating,
extra geometry, DTEMP and nonlinear initialization flags/vectors.

### BJT

Level 1 NPN/PNP, preserving three versus explicitly four terminals. No substrate
charge/current is enabled (CJS and associated physics are rejected).
Model: `IS` (1e-16 A), `BF` (100), `BR` (1), `NF/NR` (1), `TNOM`, `CJE/CJC`
(0 F), `VJE/VJC` (0.75 V), `MJE/MJC` (0.33), `FC` (0.5), `TF/TR` (0 s).
Instance: `AREA/M` (1), `TEMP`. Temperature must equal nominal.

Ebers-Moll forward/reverse transport, finite gain, both junction gmins, analytic
multi-terminal Jacobians and conserved terminal-current signs. BE/BC depletion
and diffusion charges each own a charge/derivative pair and participate in LTE.

References: `bjt/bjtload.c`, `bjtsetup.c`.
Unsupported: Early effect (`VAF/VAR`), high injection (`IKF/IKR`), leakage/bias-
dependent transit time, series resistances, substrate charge, VBIC and temperature
adjustment. They are errors, never silently discarded Gummel-Poon parameters.

### MOS1

Level 1 NMOS/PMOS. `KP` (2e-5 A/V²), `VTO` (0 V), `LAMBDA/GAMMA` (0), `PHI`
(0.6 V), `IS` (1e-14 A), `CBD/CBS` (0 F), `PB` (0.8 V), `MJ/FC` (0.5),
`CGSO/CGDO/CGBO` (0 F/m), `TNOM`, `TOX` (absent/zero only). Instance `W/L`
(100 um each), `M` (1), `TEMP`. Temperature must equal nominal.

Cutoff, triode and saturation square-law equations, channel-length modulation,
drain/source reversal and reverse-body-bias threshold effect. Analytic physical
Jacobian includes gm/gds/gmb. Bulk-source/drain depletion charges and three
constant overlap charges have **five** separate LTE-controlled state pairs.
MOS1's reverse junction current is constant below -3 Vt (not the diode/BJT law).

References: `mos1/mos1load.c`, `mos1temp.c`. In C, absent or zero TOX gives zero
oxide-capacitance factor. This is the bounded subset deliberately exercised here;
nonzero TOX/Meyer intrinsic channel charge is rejected, not approximated with a
constant capacitor. Forward-body-bias threshold/limiting with GAMMA > 0,
series resistance, area/sidewall geometry, process extraction and thermal laws
are unsupported. BSIM/CIDER/XSPICE remain outside M4.

## Solvers, sweeps and state

`NewtonOptions` defaults: 200 iterations per DC stage, reltol=1e-8, vntol=1e-10 V,
abstol=1e-12 A, maximum global nodal voltage change 0.2 V per iteration.
Global damping is a bounded policy, **not** full ngspice PN/FET limiting parity.
Unknown/duplicate/nonfinite named convergence arguments fail.

DC/AC request options include `rtol`, `vntol`, `abstol`, `maxiter`/`itl1`
(1..=10000), `gminsteps`, `srcsteps`, and `gminfactor`. Local #34 follow-ups add
`DcSettings`/`ContinuationPolicy` schedules/budgets and `solve_dc_with` success/failure reports;
OP/DC/AC bias use the same configured settings, with request > deck > defaults.
`RunConfig` still forwards physical tolerances to transient, but explicit continuation
controls reject there. See [DC_CONTINUATION.md](DC_CONTINUATION.md) for exact semantics
and differences from C's dynamic continuation. `.nodeset` seeds nonlinear bias and
is released; `.ic` is ignored in DC/AC after name validation, as in C. Configurable
junction `gmin`, other `itl*` options and full C limiting parity remain unsupported.

By default, on direct Numerical failure, DC tries nodal gmin 1e-3 down through
1e-12 S, then twenty source increments with temporary 1e-8 S nodal gmin.
Typed policies can disable or replace these schedules and bound total work. Every success
ends with **full source values and zero artificial nodal gmin**. A regularized
nonunique physical circuit still fails. Temporary stages never call accept hooks.
Linear circuits retain exact solving and source-only sweeps reuse LU factors.

Typed targets: independent V/I sources, circuit temperature and scalar resistance
(including supported model-backed resistors). One optional outer axis, inner axis
varying fastest; <=100000 Cartesian points, inclusive reachable endpoints, finite
signed steps, explicit no-progress/duplicate-target errors. Nested plots append an
explicit `sweep(outer-name)` column. Immutable point overrides preserve originals
on success/failure; R/TEMP axes rebuild numerical operators while source-only linear
axes retain LU reuse. See [DC_SWEEPS.md](DC_SWEEPS.md) for supplied/effective semantics,
analytic tests and eight opt-in live C comparisons. Arbitrary model-setter sweeps
and >2 axes remain unsupported. Local #34/#35 follow-ups are not yet published.

AC solves G(bias)+j*w*dQ/dx(bias) after a valid nonlinear DC point.
Companion transient uses the same Newton kernel, M3 breakpoint/order/work policy,
actual nonlinear Q history and **a0*dQ/dV**, not a0*Q/V. Loads and residual reloads
write only disposable trials; accept hooks run before atomic history commit.
Nonlinear `.ic`/`uic`/instance IC and the explicit diffsol nonlinear backend are
rejected. Trap/Gear orders above 2 and advanced DAE support remain unclaimed.

## Demonstrated local gate (#41)

`cargo xtask golden verify` now verifies 25 fixtures, with only subcircuit
flattening excluded. Nine exercise nonlinear devices:

| Family | DC | AC | Charge transient |
| --- | --- | --- | --- |
| Diode | existing `diode_dc` | `m4_diode_ac` (RS/CJO/TT) | `m4_diode_tran` |
| BJT | existing `bjt_ce` | `m4_bjt_ac` (CJE/CJC/TF) | `m4_bjt_tran` |
| MOS1 | existing `mos_inverter` | `m4_mos1_ac` (CBD/CBS/overlap) | `m4_mos1_tran` |

Six new pure decks were captured **individually** with the existing out-of-process
ngspice-47+ binary; no previous golden was recaptured. Twelve new parser snapshots
were added (98 existing snapshots unchanged). Ordinary tests need no C toolchain.

DC/AC: 1e-6 relative + 1e-12 absolute per real/imaginary component, allowing
independent nonlinear bias/Jacobian convergence while retaining the old, tighter
linear LU-only bounds. The existing default-tolerance diode sweep alone uses
C's default 1e-3 RELTOL + 1e-12 floor: its final stored current differs from the
physical Rust root by 0.062%, not by a new equation approximation. Independent
junction/KCL tests require the Rust point to satisfy the physical law at
1e-8 relative + 1e-12 A and explain the legacy-data error. Goldens were not moved
to make that test pass.

Transient: existing `compare::TRAN` unchanged (1e-3 relative; 1 uV / 1 pA floors),
shared physical times and one-sided breakpoint limits, never matched step arrays.
Worst fractions of the bound in the validated run: diode 0.105, BJT 0.002,
MOS1 0.187.
Each new transient compares 96 physical instants and ten breakpoint limits.

C's default saved signals omit simulator-created internal nodes: verification
projects only known `NodeKind::Internal` columns from the Rust plot, then retains
exact external variable-set/unit checks. The public Rust `sweep` scale name is
normalized to C's `v(v-sweep)`/`i(i-sweep)` naming for verification only.
No model quantities or external signals are excluded to hide an error.

`tests/m4_gate.rs` additionally pins polarity and finite gain,
MOS cutoff/triode/saturation/reversal square law, transistor DC Jacobian finite
differences, diode junction/KCL residuals, physical integrated charge at a finer
mesh, every state-pair's Q-based companion recurrence, discarded-trial/history
isolation, explicit unsupported physics, typed nested sweeps and unchanged
source values, parser fixed points, gmin rescue of a singular initial Jacobian,
source-stepping rescue of an iteration budget and rejection of a regularized
nonunique final circuit. Existing M1/M3, solver and accept-failure regressions
remain required. This is local evidence for the demonstrated subset, not a remote
issue closure or a claim about unsupported physical regimes.

## Validation

Final local run: **587 passed / 29 opt-in tests ignored**, independently on stable
and Rust 1.89.0; all-target Clippy with warnings denied passes on both. All twelve
M4 physical/continuation/ownership tests pass. Formatting and diff whitespace
checks pass; 110 snapshots are unchanged; Rust verified 25/26 goldens at this
M4 gate (only subcircuit flattening was excluded; #18 has since closed that gap
and the current gate verifies all 26), and the external C drift check
reproduces all 26 golden files. The six newly captured M4 goldens are included with the
implementation for review. Remote #41 remains open pending review; unsupported
physics remains outside this local gate.

Run inside this worktree:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo +1.89.0 test --workspace --locked
cargo +1.89.0 clippy --workspace --all-targets --locked -- -D warnings
cargo xtask golden verify
cargo xtask snapshots
```

C drift checks (no fixture recapture):

```sh
NGSPICE_BIN="$PWD/../../ngspice_test/build/src/ngspice" cargo xtask golden check
```
