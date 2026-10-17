# TXL: the single lossy transmission line (`Y`)

`Y` instances with a `txl` model are the `TransLine` device of
`src/spicelib/devices/txl/` (`txlinit.c`, `txlsetup.c`, `txlload.c`,
`txlacct.c`, `txlparam.c`, `txlmpar.c`, `txlask.c`, `txlfbr.c`), parsed by
`parser/inp2y.c` and prepared by `inp2p.c` when the model is a `CplLines`
type. The port's implementation is `src/devices/txl.rs` (with
`src/devices/txl/pade.rs` and `src/devices/txl/transient.rs`) and
`src/netlist/parser/txl.rs`. Registry status: **bounded** (#170–#173).

```text
Yname n1 g1 n2 g2 model [len=<value>]
```

Four terminals are written and the second and fourth are **references**: the
Padé device binds only `node1` and `node2` (`bindNode(fast, 1, node1)`,
`bindNode(fast, 2, node2)` in `INP2Y`), which is why a deck may write `0`
twice and see no effect from the reference nodes. The two signal terminals
become the device's `TXLposNode`/`TXLnegNode`.

## Two paths

`INP2Y` decides at *parse* time which device a card becomes, and the two have
nothing in common afterwards.

### 1. The three-pi fallback (a parser-time expansion)

Trigger, as C evaluates it by re-lexing the model card's own line:

```c
if (lenval && rval && lval && rval/lval > 1.6e10)
```

so it is the **ratio `R/L`** (ohms per henry), not `R*length`, that decides.
`lenval` is the instance `len` if written, else the model's `length`.

C then builds, in name order:

| Element | Instances | Value |
| --- | --- | --- |
| series resistors | `txlres1<name>` … `txlres3<name>` | `3/(R*length)` |
| shunt capacitors | `txlcap1<name>` … `txlcap4<name>` | `c*length/6`, `2c*length/6`, `2c*length/6`, `c*length/6` |

between `node1 → txlnd1<name> → txlnd2<name> → node2`, with every capacitor
returning to **`gnode1`**, the *first* reference terminal. The `Y` instance
itself is not created on this path, and the generated names are visible in
rawfiles (`v(txlnd1y1)`) and as ordinary `@txlres1y1[resistance]` asks.

Two C quirks are reproduced literally rather than corrected:

* `3.0/(rval*lenval)` is dimensionally a **conductance** but is handed to the
  resistor's *resistance* setter, so a fallback ladder is a near short. The
  port's comments and tests state this instead of "fixing" it.
* The model is still validated by `TXLsetup` *after* the expansion, so a
  fallback deck must give all five parameters (`g` included) or the run fails
  with "lossy line parallel conductance not given" even though no `Y`
  instance exists. The port's schema enforces the same five.

Because the trigger needs `R/L > 1.6e10` while the ladder resistance is
`3/(R*length)`, a fallback line can be extremely stiff: `m10_txl_fb_op`/
`m10_txl_fb_dc` choose `r=1e9 l=1e-2 length=3e-11` so the sections are
100 ohm and the DC divider is well conditioned. C's rawfile omits none of the
generated nodes, so they are compared.

### 2. The Padé path

`ReadTxL` (in `txlsetup.c`) has two shortcuts and otherwise builds a Padé
approximation:

* `R/L < 5e5` **and** `G < 1e-2` → a lossless line (`lsl`): `taul = sqrt(LC)*l`,
  `atten3 = sqtCdL = sqrt(C/L)`, `atten2 = 1`, no `h1` terms.
* `R/L >= 5e5` → `line->g = 1/(R*l)` (a field only the LTRA/CPL machinery of
  the original code reads).
* otherwise `main_pade` → `y_pade` (the characteristic admittance
  `sqrt((1+az)/(1+bz))` as three partial fractions) and `exp_pade` (the
  propagation function `exp(-length*sqrt((sL+R)(sC+G)))`, whose complex
  conjugate pair is carried as one term with `ifImg = 1`), then `get_h3`
  (the product `H1*H2`'s terms) and `update_h1C_c`.

The cubic solvers (`find_roots`, `exp_find_roots`) are Cardano seeds with at
most 31 Newton updates and a stable deflation; two Gauss-Jordan solves follow
C's two pivot thresholds (`1e-16`, `1e-28`) and its partial-row swaps.
`pade.rs` keeps C's operation order so the coefficients are bit-compatible,
but returns `Numerical` errors where C calls `controlled_exit`.

`m10_txl_op`/`m10_txl_dc` pin the DC answer and `m10_txl_tran`/
`m10_txl_early_tran`/`m10_txl_tran_step`/`m10_txl3_tran` the transient.
`m10_txl_lossless_tran` covers the lossless shortcut; `m10_txl_fb_tran`
covers the generated ladder's charging response.

## The DC load (`TXLload`, `MODEDC`)

Two branch currents are allocated (`CKTmkCur(..., "branch1"/"branch2")`), so a
`Y` instance has **four unknowns**, numbered after the nodes in C's setup
order (`txl` sits between `tra` and `vbic` in `dev.c`'s device table).

The `cond1` branch stamps `+i1` in the input node's KCL and `+i2` in
the output node's KCL, with two branch equations and no RHS:

```text
i1 + i2 = 0
v(pos) - v(neg) - R*length*i1 = 0
```

It also adds `+0.1*gmin` to all four node-node entries (including both
off-diagonal entries), as C does. The result is
`v(pos) - v(neg) = R*length*i1`: a resistive line with `length` resolved
instance-first (`TXLlengthgiven`, else the model's `length`).

C's rawfile names **both** branch currents `i(y1)` (`raw_write` strips
`CKTmkCur`'s suffix), and the port's plot does the same. The comparison
identifies a repeated name by occurrence, so the two are still checked
position by position (`xtask/src/compare.rs`).

## The transient (`txlload.c`, `txlacct.c`)

The Padé terms are convolved recursively. Per accepted point C appends one
sample of `[v_in, v_out, i_in, i_out]` (integer picoseconds, `(int)(t*1e12)`),
and each load recomputes the branch RHS from the samples around
`t - taul`: `get_pvs_vi_txl` interpolates `[v,i]` at `ta` and `tb`, and
`right_consts_txl` updates the `h1`/`h2`/`h3` "cnv" accumulators with
`exp(x*h)` factors. `lsl` lines use the plain `atten3*v + atten2*i` form.

The port keeps each accepted sample's *whole* convolution state in the
device's delay history, so a Newton trial is pure: nothing a rejected trial
does can change the convolution, the interpolation cursors or the history.
That is the port's own contract (`docs/port/TRANSIENT.md`) applied to a
device whose C state is a mutable pair of `TXLine` snapshots.

Three C behaviours are reproduced deliberately:

* **`h1_term` integer arithmetic.** `update_cnv_txl` computes
  `delta = time - before` in *integer picoseconds* and reuses-and-rescales
  `bi`/`bo` inside its loop; the port keeps that order rather than the
  analytically equivalent trapezoidal form.
* **`update_delayed_cnv_txl`.** The `ratio` extrapolation for the part of the
  delayed sample that reached past the last accepted point is added to `h2`
  and `h3` with C's `0.5e-12` scaling.
* **The step clamp.** When a load's `h` exceeds `taul`, C prints "your time
  step is too large for TXL tau." and sets `CKTmaxStep = 0.9*taul`, so the
  **first** oversized step is taken and the following ones are bounded. The
  port's `DelayLine::timestep_limit` offers `0.9*taul` only once an accepted
  sample gap already exceeded `taul`, which reproduces that ordering
  (`m10_txl_tran_step`: `tmax = 10 ns` against `taul = 5 ns`, 34 accepted
  points on both sides). A 7.168 ns gap triggers the clamp; subsequent gaps
  are at most 4.5 ns. An accepted-state flag preserves the clamp after the
  triggering gap leaves the pruned history.

`TXLaccept` (which records node slopes for the next load) has no analogue:
the slopes are recomputed from the history when needed, so nothing is
committed at acceptance beyond the sample itself.

## Refused analyses

These boundaries apply to the Padé device. On the fallback path the `Y`
is absent: ordinary R/C devices supply their own DC/AC/transient behaviour.
`uic` is explicitly refused for the Padé path (no initial-wave setters),
and transient times beyond the signed i32 picosecond range (about 2.147 ms)
fail explicitly instead of reproducing C's undefined float-to-int overflow.
Invalid domains and nonfinite derived coefficients/delays are errors.

| Analysis | C | Port |
| --- | --- | --- |
| `.op`, `.dc` | `TXLload` `MODEDC` | supported |
| `.tran` | `TXLload` convolution | supported |
| `.tf` | `tfanal.c` reuses the operating point's factors | supported (the DC line) |
| `.ac`, `.sp` | `DEVacLoad = TXLload`, whose `CKTdelta` is never assigned by `niaciter.c`/`acan.c` | `NotYetPorted` |
| `.pz` | `DEVpzLoad = NULL` | `NotYetPorted` |
| `.noise`, `.disto` | `DEVnoise`/`DEVdisto = NULL` | `NotYetPorted` |
| `.sens` | no `DEVsen*` | `NotYetPorted` (see #164) |

The AC refusal is TX0's policy decision, not an unimplemented feature: C's
answer there is frequency-independent residue. Measured on the reference
binary, a `r=12.45 l=8.972n c=0.468p length=16` line with `V1 ac 1` gives
`vm(out)` = 1.3846e-11 V at both 1 MHz and 1 GHz, and a 50 ohm line gives
2.5e-12 V; a physical 5 ns line would pass about 0.99. The port refuses
rather than reproducing that.

`.tf` is deliberately *not* on that list, and this is worth stating because
the split is not obvious: `.tf` solves with the operating point's factored
matrix, so it answers `R*length` exactly as C does (the port matches C to
1.4e-16 on `m10_txl3_tran`'s sibling `.tf` deck).

## Asks

`TXLask` exposes three entries, of which only `length` is reachable from the
front end (`in_node`/`out_node` are `IF_NODE` entries C's own `@y1[...]`
lookup refuses: "no such parameter in_node"). `@y1[length]` reports the
**instance** `TXLlength` field, which stays 0 unless `len=` was written - a
model-level `length` therefore reads as 0 (verified against the reference
binary). The port does the same.

## Verification

| Fixture | Gate | Result |
| --- | --- | --- |
| `m10_txl_fb_op`, `m10_txl_fb_dc` | `DC` | 1 + 11 points |
| `m10_txl_fb_tran` | `TRAN` | 1186 points, generated ladder's charging response |
| `m10_txl_lossless_tran` | `TRAN` | 254 points, matched lossless line |
| `m10_txl_op`, `m10_txl_dc` | `DC` | 1 + 11 points |
| `m10_txl_tran`, `m10_txl_early_tran` | `TRAN` | 214 points each |
| `m10_txl_tran_step` | `TRAN` | 34 points (the `0.9*taul` clamp) |
| `m10_txl3_tran` | `TRAN` | 495 points, three lines, MOS drivers |

`tests/txl.rs` adds thirteen C-free tests (grammar, the five required parameters,
CPL refusal, the fallback ladder and its trigger, the Padé DC relation
`v(p)-v(n) = R*length*i1`, the `@y1[length]` rule, the accepted-point counts,
the step clamp, the refusals and `.tf`, invalid/overflow domains, the lossless
shortcut and the ladder's analytic first moment). Unit tests also check
rejected-trial purity, clamp persistence after pruning, restart and the
picosecond range. For the 100 ohm/2 pF/2 pF/1 pF ladder, nodal elimination
gives `H(s) = 1/(1.3 + 9.8*x + 12.4*x^2 + 4*x^3)`, `x=s*1e-10`;
the normalized step-response deficit integrates to `9.8/1.3*1e-10 s`
(plus half the source rise time), checked independently of C.

### Measured limits

* **Source-driven decks agree to last-bit precision.** `m10_txl_tran` and
  `m10_txl_early_tran` reproduce C's 214 accepted points with a worst
  relative difference of 5.3e-16. The step-clamp fixture also reproduces C's
  accepted-point sequence. This is bounded fixture evidence, not a proof of
  equivalence for every coefficient regime.
* **The line amplifies step-size differences.** With a nonlinear driver
  (`m10_txl3_tran`, terminated) the agreement is 1.4e-8 relative, because the
  port's and C's accepted times differ in the last bits (7.9e-16 s over the
  run). The recursive convolution's factors are `exp(x*h)` with `|x|` of
  order `1/tau`, so a step difference of that size moves them relatively.
  Sensitivity to step size is also measurable in C alone (consistent with,
  but not proof of, this explanation): changing the upstream deck's `tmax` by one ulp changes its `v(2)` by
  up to 4.0e-7 V (measured: `.TRAN 0.2N 47N 0 0.1N` against
  `0.0999999999999999N`). The unmodified upstream `txl2_3_line.cir` deck
  agrees to 1.4e-8 relative on its voltages as well.
* **Floating far ports make the currents meaningless.** In those upstream
  decks the far line ends are loaded only by a capacitor, so their DC currents
  are numerical residue (about 1e-12 A, and 2.5e-9 A during the early
  transient) which C and the port resolve differently. `m10_txl3_tran`
  therefore adds a 1 kohm termination per line, which makes the deck's
  currents physical (mA) and the whole comparison meaningful.
* **C's own stiff fallback is not a fixture.** A literal `r=1e12 l=1
  length=1` fallback creates 3e-12 ohm sections; the port's node voltages
  then match C bit for bit but `i(v1)` differs by 1.2% because it is the
  cancellation of two 5 V values that differ in their last bits
  (`-4.8828125e-3` against C's `-4.94384765625e-3`). Both are artefacts of
  the conditioning, so the well-conditioned ladder above is the fixture.
* **One oversized first step.** C accepts the step that triggers the clamp;
  the port offers the bound only afterwards, so it also accepts it. Because
  the port's estimate is consulted on the converged trial rather than during
  the load, a deck whose *first* step is oversized behaves identically only
  because that bound is withheld until an oversized gap exists in the
  history.
