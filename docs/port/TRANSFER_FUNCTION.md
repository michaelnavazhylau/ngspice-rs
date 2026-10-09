# `.tf` transfer-function analysis (#101)

`.tf outvar insrc` computes the DC small-signal gain from an independent source
to an output, the resistance the source sees and the resistance seen at the
output, all at the DC operating point. The driver is `analysis::tf`, reached as
`AnalysisKind::TransferFunction` through `analysis::runner` and therefore through
`spice-rs simulate`, multi-analysis decks and `cargo xtask golden verify`.

C references (behaviour only, reimplemented): `TFanal()` in
`src/spicelib/analysis/tfanal.c`, `TFsetParm()` in `tfsetp.c`, `dot_tf()` in
`src/spicelib/parser/inp2dot.c`, and the rawfile naming of `raw_write()` in
`src/frontend/rawfile.c`.

## Syntax

| Card | Output | Input |
| --- | --- | --- |
| `.tf v(n) insrc` | `v(n)` | independent `V` or `I` source |
| `.tf v(n,m) insrc` (comma optional) | `v(n) - v(m)` | independent `V` or `I` source |
| `.tf i(src) insrc` | branch current of `src` | independent `V` or `I` source |

`i(src)` accepts every device with a findable branch current, as
`CKTfndBranch()` does: independent voltage sources, E and H sources and voltage
B sources (including E forms the port lowers onto them). Inductor currents are
not findable, as in C. Deck
`.option` DC settings (`reltol`, `vntol`, `abstol`, `itl1`, `itl2`, `srcsteps`,
`gminsteps`, `gminfactor`, `noopiter`) and `.nodeset` apply to the bias point as
for `.op`; `.ic` does not (C enforces it only in a transient operating point).

## Algorithm

1. Solve the operating point exactly as `.op` does (`CKTop`).
2. Reload the circuit once at that point (`MODEINITFLOAT`, previous iterate =
   the converged one, so no junction limiting acts) and factor the matrix once.
   The load asks for C's own `DEVload` matrix
   (`TrialState::with_c_jacobian`): the port's Newton load is deliberately exact
   for the BJT's bias-dependent base resistance (`RB`/`RBM`/`IRB`), while
   `bjtload.c` stamps the conductance `gx` alone. `.tf` reproduces C, which also
   makes it agree with `.ac` at low frequency. Switches keep `swload.c`'s state
   rules (only `REALLY_ON`/`HYST_ON` conduct), not the `SWacLoad` rule `.ac` uses.
3. Solve a unit excitation of the input: `rhs[branch] += 1` for a V source;
   `rhs[n+] -= 1; rhs[n-] += 1` for an I source (1 A through it). The output in
   that solution is the transfer function. The input resistance is `-1/i(insrc)`
   for a V source (`1e20` when `|i| < 1e-20`) and `v(n-) - v(n+)` for an
   I source.
4. Reuse the factors for the output resistance: draw 1 A out of `n` into `m`
   (`rhs[n] -= 1; rhs[m] += 1`) and report `v(m) - v(n)`, or apply a unit voltage
   in the output branch and report `-1/i` (`1e20` when `|i| < 1e-20`; the
   reference binary's Enhancement-179 fix of SPICE3's clamp). When the output
   current is the input source's own, the input resistance is copied, as C does.

## Plot

One `Transfer Function` plot (`tf1`, `tf2`, ... in a batch; ngspice runs `.tf`
after `.ac`, `.dc`, `.op`, `.tran` and `.pz`, same-type cards in reverse deck
order), flagged `real`, with one point and three `voltage` vectors named as
`ngspice -b -r` writes them:

| Column | Name |
| --- | --- |
| transfer function | `v(Transfer_function)` |
| input resistance | `v(<insrc>#Input_impedance)` |
| output resistance | `v(output_impedance_at_V(<n>))`, `v(output_impedance_at_V(<n>,<m>))` or `v(<src>#Output_impedance)` |

Names are lowercased (C lowercases the deck) and nodes are canonical, so `gnd`
is written `0` under the ground alias. ngspice's `.control` `write` route, used
by `cargo xtask golden capture`, lowercases the whole name and reorders the
vectors; golden comparison is by case-insensitive name.

`.save` applies to every plot, and none of the node names it can request exist
in a `.tf` plot, so a deck combining `.save v(x)` with `.tf` fails explicitly
(C reports "no data saved for transfer function analysis; analysis not run" and
aborts the run). `.print tf v(Transfer_function)` selects a `.tf` vector.

## Deliberate divergences

Each is an explicit error where C would carry on:

* C ignores whether `CKTop` converged and solves with whatever matrix is left;
  the port propagates the operating-point failure.
* C creates an unknown output node on the fly and reports values for that
  isolated row (the reference binary writes a 0 gain and a 1 ohm output
  resistance); the port reports the unknown node.
* For `i(x)` where `x` has no findable branch, C reads row 0 (ground) and
  reports a zero transfer and `1e20` resistance; the port rejects the card.
* C ignores tokens after the input source; the port rejects them.
* An input that is not an independent V or I source, or is missing, is an error
  in both.

## Verification

* `tests/analysis_tf.rs`: closed-form dividers (with an inductor short and a
  capacitor open), differential and ground-aliased outputs, current-source
  inputs and their sign rules, sensed current outputs, the same-source copy,
  C's `1e20` open resistance, a finite-gain E amplifier against a nodal
  reference solution, diode/BJT/MOS1 gains and input conductances against
  central differences of `.op`, the BJT `gx`-only matrix against low-frequency
  `.ac`, batch order and plot names, deck DC options and every error path.
* C goldens `m8_tf_divider`, `m8_tf_controlled` (linear `compare::DC`),
  `m8_tf_bjt` (`compare::NONLINEAR`, tightened RELTOL, bias-dependent base
  resistance) and the batch `m8_tf_batch` (`.op` plus two `.tf` plots, diode
  and MOS1) in `cargo xtask golden verify`.
* Opt-in `tests/c_tf_reference.rs` runs `ngspice -b -r` and `spice-rs simulate`
  on passive, current-input, E/F/G/H, nonlinear (diode, IRB BJT, MOS1 with
  `.op`/`.ac`) and S/W switch-in-hysteresis decks and requires exact vector names
  and values within `1e-9` (linear) or `1e-6` (nonlinear) relative.
