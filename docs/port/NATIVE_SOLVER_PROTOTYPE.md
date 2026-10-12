# Native-solver prototype (`proto/native-solver`)

Prototype branch that removes the `faer` and `diffsol` dependencies and uses
ngspice's own numerical methods, ported to Rust. Purpose: measure what C-style
numerics buy in speed and fidelity before deciding what to merge. Not merged;
see [Decisions to make](#decisions-to-make).

## What changed

| Area | Before (`work/m9`) | Prototype | C reference |
| --- | --- | --- | --- |
| Sparse real/complex LU | faer sparse LU + n-solve rank probe + backward-residual checks | `src/maths/sparse13.rs`: port of SPARSE 1.3 (Markowitz, diagonal/threshold pivoting, fill-in, MNA preorder, transpose solve, determinant) | `src/maths/sparse/sp*.c`, `spsmp.c` |
| Pivot reuse | none (fresh factor every solve) | Newton iterations, linear transient steps and AC frequencies refactor with the previous pivot order (`spFactor`), reordering after a zero pivot | `NIiter`, `NIacIter`, `SMPluFac`/`SMPcLUfac` |
| Newton linear solve | row-equilibrated, Curtis-Reid fallback | raw matrix, threshold pivoting | `niiter.c` |
| Dense LU | faer partial pivoting | through the SPARSE port | — |
| Symmetric eigenvalues (K check) | faer | cyclic Jacobi | (C uses Cholesky and only warns) |
| Pole-zero | SVD deflation + faer QZ | port of the deflated Muller search | `cktpzstr.c`, `nipzmeth.c`, `SMPcDProd` |
| `.tran` backends | companion (default) + `backend=diffsol` BDF | companion only; `backend=diffsol` is an explicit `Unsupported` | `dctran.c` |
| Circuit build | node table cloned per R/C/L/V/I instance (O(n²)) | intern in place with rollback | — |
| Stamping | per iteration: push triplets, sort/fold, compare pattern, copy factors | matrix bound to a circuit `Pattern` of `ElementId(u32)` elements numbered in first-stamp (device) order; stamps replay a per-device trace (`values[id] += v`), falling back to a per-row lookup; one pivot order persists per pattern; factors refactored in place | `TSTALLOC`/`spGetElement`, `spClear`, `*ptr += v` |
| Charge history | heap `Vec` per junction/capacitor per load | stack `ChargeHistory` | — |

Dependencies: the lockfile goes from ~200 packages to 18 (`winnow`, `petgraph`
and their deps). SPARSE 1.3 carries a permissive Berkeley notice (reproduced
in `sparse13.rs`); nothing is taken from LGPL KLU.

## Fidelity to C, and known divergences

- **Pivot sequence.** SPARSE's internal numbering follows element creation
  order. Circuit loads now create elements in first-stamp (device) order, close
  to ngspice's `TSTALLOC` order; standalone `SparseMatrix` solves (tests, small
  helper systems) still use folded row-major order. Markowitz ties can break
  differently from C, changing rounding only.
- **Element creation order (open).** C does not stamp in deck order:
  `CKTsetup` walks device types in `DEVices` order, and within a type
  `CKTmodCrt`/`CKTcrtElt` prepend models and instances, so the last
  instance card sets up first. SPARSE numbers rows by first appearance, so
  C's internal numbering, and with it Markowitz tie-breaking in
  `QuicklySearchDiagonal`, differs from this port's deck-order pattern. Seen
  with `.options pivrel=1e-30`: on a deck where two diagonals tie at
  Markowitz product 1, C (with `.options sparse`) keeps the well-conditioned
  one and this port takes the 1e-17 pivot. With the default `pivrel` both
  reject it. Matching C means learning the pattern in C's setup order.
- **Pivot persistence.** One pivot order is kept per circuit pattern and reused
  by every solve, as C keeps one ordered matrix; a new element (a position no
  previous load stamped) forces a reorder, as in SPARSE. Elements are learned
  from loads rather than allocated at setup, so a position first stamped later
  (e.g. capacitor companions at the first transient load) reorders once.
- **Error policy.** As in C, ill-conditioned systems are accepted (small
  pivots are `OK`); only structural singularity, exact zero pivots and
  non-finite solutions fail. This **removes the rank-diagnostic guardrail** in
  `AGENTS.md` and `SPARSE_RANK_DIAGNOSTICS.md`. Adopted by M10 item 0.5a:
  singular errors name the unknown as `NIiter` does (`singular matrix:  check
  node X`), small pivots print a warning, an unknown with no matrix element
  keeps its right-hand side as in C, tiny resistances are clamped to `RESMIN`
  and zero C/L accepted (`tests/solver_robustness.rs` checks each against C).
- **Pole-zero.** The search is C's algorithm, including its failure modes
  (iteration limit, giving up). It is sensitive to rounding: on
  `multi_analysis_pz.cir` the port reaches the real pole at -7.97e4 via Muller
  (imaginary part ~1e-36, so it is listed as a pair: 4 poles) where C reaches
  it on the real axis (3 poles). The other roots agree with C to 12–16 digits;
  the notch deck now reports no poles, as C does.
- **Newton.** The diode-chain `.op` failure seen in the baseline is no longer a
  rank-diagnostic rejection but a Newton iteration-limit failure; it is a
  convergence-control difference outside the solver.

## Benchmarks (C ngspice-47 KLU `-O2` vs Rust release, Apple M5)

`scripts/bench/bench.py` decks, outputs checked vector by vector against C.
The machine was shared and heavily loaded, so the table uses CPU time
(user + sys) ratios, Rust / C; instructions retired (`/usr/bin/time -l`) were
used for A/B decisions because they do not depend on load.

| Deck | Baseline (`work/m9`) | Solver swap | + bound stamping |
| --- | --- | --- | --- |
| RC ladder 1k `.op` | 7.9 | 1.3 | 1.6 |
| RC ladder 10k `.op` | 101 | 2.1 | 2.6 |
| Resistor mesh 10k `.op` | 4.8 | 1.2 | 1.2 |
| Resistor mesh 50k `.op` | timeout | — | 1.1 |
| RC ladder 1k `.dc` (1001 pts) | 1.7 | 0.6 | 0.5 |
| RC ladder 1k `.ac` (901 pts) | 594 | 3.4 | 3.4 |
| RC ladder 10k `.ac` | timeout | 2.1 | 3.7 |
| RC ladder 1k `.tran` | 275 | 3.4 | 3.3 |
| RC ladder 100 `.tran` 200k pts | 40 | 4.2 | 2.3 |
| MOS1 inverters, 102 FETs `.tran` | 16 | ~9 | 6.4 |
| MOS1 inverters, 402 FETs `.tran` | 32 | ~11 | 7.0 |
| Diode chain 100 / 1k `.op` | fails | fails (Newton) | fails (Newton) |

The middle column is wall time from a quieter run, so read the last two
columns as approximate. The bound-stamping step alone, measured A/B on one
binary by instructions retired, removed 8–15% of the work on transient decks.

### Where the remaining MOS gap is

On the 102-FET inverter chain Rust and C take the **same** timesteps (2002/2003
accepted, 530 rejected each), but Rust performs 10,400 device loads against C's
8,110 Newton iterations (an extra reload per converged point for the residual
check and to record charges), and each load+solve costs ~1.1M instructions
against C's ~160k. The profile is now device evaluation, not the solver:
`Mos1::stamp`, `Coefficients::integrate`, per-call `finite` validation,
`DeviceState::set`, `stamp_junction`, plus residual allocation and
`SparseMatrix::add` bookkeeping. Next steps in that order: drop the extra
reloads where C has none, move validation out of the per-iteration path, and
remove the remaining per-load allocations. Plot memory (915 MB for the
200k-point transient) is unchanged and needs streaming output.

## Test impact

The suite was updated for the intentional behaviour changes: BDF-backend tests
and the `diffsol-bdf` golden-verify variants were removed, rank-diagnostic
rejection tests were removed or narrowed to exact singularity, bit-exact float
assertions became tolerance checks, and the two pole-zero tests that encoded
eigenvalue-method behaviour now encode C's. Production C-golden fixtures
(companion variants) all pass unchanged.

## Decisions to make

1. Adopt C's error policy (drop the rank diagnostics) or keep a cheap
   condition estimate on top of SPARSE.
2. Remove `backend=diffsol` permanently, or keep diffsol as an optional
   feature for index-one DAE work.
3. Pole-zero: C's search (fidelity, including its failures) or the eigenvalue
   method (robustness).
4. Follow-ups if adopted: device-level evaluation cost (above), reading
   `.options pivtol/pivrel` (done: `AnalysisContext::pivot` reaches every
   circuit factorization), streaming rawfile output, and whether to remove the
   remaining `backend=diffsol` option checks in `src/analysis/config.rs`.
