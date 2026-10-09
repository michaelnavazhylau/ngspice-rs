# ADR #103: pole-zero analysis by pencil eigenvalues

Status: **accepted and implemented** (`.pz`, milestone M8). Code:
`src/analysis/pz.rs` (card, C's drive/column conventions, plot),
`src/maths/pencil.rs` (roots of `det(A + s E)`), `Device::assemble_pole_zero`
(per-device loads). Tests: `tests/pole_zero.rs`, opt-in `tests/c_pole_zero.rs`,
the `pz_*` and `multi_analysis_pz` goldens in `golden verify`, unit tests in both
modules.

## Context: what C computes

```text
.pz in+ in- out+ out- {cur|vol} {pol|zer|pz}
```

Read-only behavioural references: `src/spicelib/analysis/pzan.c` (`PZan`,
`PZinit`, `PZpost`), `cktpzset.c` (`CKTpzSetup`), `cktpzld.c` (`CKTpzLoad`),
`cktpzstr.c` (`CKTpzFindZeros`: the root search), `src/maths/ni/nipzmeth.c`
(Muller step, deflation), the device `*pzld.c` loads and `parser/inp2dot.c`
(`dot_pz`) with `analysis/pzsetp.c`.

`PZan` solves the operating point (`CKTop`), reloads with `MODEINITSMSIG` and,
for poles and then zeros, searches for the zeros of a determinant:

1. Every device `DEVpzLoad` stamps `Y(s)`. Each load is **affine in `s`** and
   equals the AC load with `j omega` replaced by `s` (capacitor `s C`, inductor
   branch `-s L`, mutual `-s M`, junction/Meyer charges `s dQ/dV`), with one
   exception: an independent voltage source **with an AC value** (`VSRCacGiven`,
   also `ac 0`) is removed — its branch row becomes `i = 0` while its KCL
   couplings stay (`vsrcpzld.c`). A voltage source without an AC value shorts
   its terminals as in AC. Current sources have no pole-zero load (open).
2. `CKTpzSetup`/`CKTpzLoad` drive the input with a unit current and replace one
   column. For zeros the *solution column* is the output node; for `vol` poles
   it is the input node; `cur` poles use `Y` unmodified. A non-ground negative
   output first has the solution column added to its own column (`SMPcAddCol`),
   so the unknown becomes the differential output voltage. With a grounded
   positive output the negative node is the column and the drive is swapped.
   The solution column is zeroed and receives `+1`/`-1` in the input rows.
3. By Cramer's rule the modified determinant is `det Y(s)` times the
   transimpedance from the input current to the solution voltage, so its zeros
   are the zeros of `Vout/Iin` (`zer`), of `Vin/Iin` (`vol pol`: the poles of
   `Vout/Vin`) and of `det Y` (`cur pol`).
4. `CKTpzFindZeros` searches the real axis outward from 0 with bracketing
   steps, Muller iterations in the complex plane and deflation by the roots
   already found (`NIpzK`), until it runs out of trials (`NITER_LIM = 200`),
   finds `SMPmatSize` roots or its guesses exceed `1e35`. It keeps
   upper-half-plane roots; `PZpost` writes each complex root followed by its
   conjugate, in the list order (ascending real part, then imaginary part).

`PZinit` refuses a shorted input or output, a `vol` transfer of unity or `-1`
and any transmission line; a determinant that vanishes at every trial is
`E_SHORT` "The input signal is shorted on the way to the output".

## Decision: finite generalized eigenvalues of the modified pencil

Since every C pole-zero load is affine in `s`, the modified matrix is a matrix
pencil `M(s) = A + s E` and its determinant roots are exactly the **finite
generalized eigenvalues** of `(A, -E)`. The port computes them directly instead
of porting the Muller search. Alternatives considered:

| Option | Assessment |
| --- | --- |
| Port C's deflated Muller search | Reproduces C's failures (below) and its strategy constants; root accuracy and completeness depend on the search path; requires a complex determinant with exponent tracking. Rejected. |
| QZ of the whole pencil (LAPACK `dggev` style) | Accurate when the infinite eigenvalues are semisimple, but MNA pencils routinely have index-two blocks at infinity (a capacitor across an ideal source, inductor/current-source cutsets), which QZ perturbs into spurious finite eigenvalues of size `~1/sqrt(eps)`. Classifying them by `|beta|` is a threshold guess. Rejected as the sole method. |
| Schur-complement elimination of the algebraic block, then eigenvalues | Implemented first and measured: the eliminated algebraic blocks have condition numbers of `1e11` and more (gmin, `rbm`, `re` next to `kohm` resistors), and a BJT amplifier's poles lost up to 8 % (unbalanced) and produced a spurious `-1e22` zero (balanced). Rejected. |
| **Orthogonal staircase deflation of the infinite eigenvalues, then QZ** | Backward stable (orthogonal transforms only, no block inverted), removes infinite eigenvalues of any index by rank decisions, leaves a pencil with nonsingular `E` whose QZ eigenvalues are the roots. **Chosen.** |

`maths::pencil::pencil_roots` therefore:

1. scales exactly (powers of two): Ruiz row/column equilibration of `[A E]`,
   interleaved with a frequency scale `omega` balancing `||A||` and
   `||omega E||` (undone on the roots). Without it the reduction lost digits
   on the same BJT pencil;
2. repeats: SVD row compression `U^T E = [E1; 0]`; the `s`-free rows of
   `U^T A` must have full row rank (else the pencil is **singular**, `det == 0`
   for every `s`: an explicit error); their SVD column compression makes the
   pencil block upper triangular `[[A11 + s E11, *], [0, A22']]` with a constant
   nonsingular `A22'`, so the `r x r` pencil `(A11, E11)` keeps every finite
   root; a singular `E11` is a higher-index block and the loop repeats;
3. computes the eigenvalues of the final `(A, -E)` with faer's complex QZ
   (`faer::linalg::gevd::gevd_cplx`, eigenvalues only);
4. restores conjugate symmetry: roots are paired greedily by the smallest
   `|z_i - conj(z_j)|` (a root paired with itself becomes real); a pairing
   distance above `1e-6` of the largest root magnitude is reported as an error,
   never symmetrised away.

Rank decisions use one tolerance, `16 n eps max(||A||_F, ||E||_F)` on the
scaled pencil. The pencil is dense; dimensions above 1000 unknowns are refused
explicitly (`MAX_PENCIL_DIMENSION`). No dependency was added: faer 0.24.4 (MIT,
already the production LU backend) provides SVD and QZ.

Two faer 0.24.4 paths are avoided deliberately and documented in the code: the
high-level `Mat::generalized_eigen` also computes eigenvectors and under-sizes
that workspace for small pencils (it panicked for dimensions one and two), and the real
QZ (`gevd_real`) returned non-conjugate eigenvalues for a series-RLC 2x2 block
(both off by about 4 %). The port's roots agree with LAPACK QZ of the full pencil (`scipy.linalg.eigvals`,
on decks without index-two blocks) to `1e-10` relative or better on every deck
checked, roots at the origin excepted.

## Divergences from C

* **Completeness.** C's search can stop early with only a warning ("Pole-zero
  iteration limit reached", "converging to numerical aberrations") and report
  a subset — for a three-element notch (`tests/pole_zero.rs`) it reports none
  of the three poles, and for a Gummel-Poon amplifier four of seven poles. The
  port has no search and reports every finite root. The goldens use decks on
  which C finishes without a warning; the live C test asserts C finished.
* **Accuracy.** C stops at its trial-coincidence tolerances (`reltol = 1e-6`
  for roots in `CKTpzRunTrial`); the port's roots are backward stable. The two
  agree to `1.5e-9` relative or better on the goldens. A root C reports as exactly
  `0` (its LU is singular there) is a value of order `eps` times the pencil
  scale in the port. An exact double root splits by about `sqrt(eps)` relative,
  into two close real roots or a narrow complex pair.
* **Ordering.** The port writes C's order (ascending real part, then imaginary
  part; conjugate after its root). Ties between perturbed multiple roots may be
  listed differently, so comparisons treat the roots as unordered sets.
* **CCVS sign.** C's `CCVSpzLoad` (`ccvspzld.c`) adds the transresistance to
  the branch row with the opposite sign of `CCVSload`/AC, so C analyses a deck
  with an H source as if its gain were negated. The port uses the AC load (the
  circuit as written); `c_pole_zero.rs` pins that it matches C's result for the
  negated gain. All other C pole-zero loads of supported devices equal their AC
  loads.
* **Singular pencils.** When `det M(s)` vanishes identically (output not
  reachable from the input, an input shorted by a source without an AC value)
  C fails with `E_SHORT`; the port fails with a numerical error naming the
  singular pencil and C's message.
* **Unknown nodes.** C's parser creates a node the deck never uses (an
  isolated row that makes the determinant vanish); the port refuses the card.
* **Card flags.** C matches `cur`/`vol`/`pol`/`zer`/`pz` by name in either
  order and silently keeps defaults for a missing flag; the port accepts either
  order but requires exactly one input type and one selection.
* **Empty result.** Like C, a circuit without finite roots writes a plot with no
  variables, one point and `Flags: real`.

## Device support

Opt-in through `Device::assemble_pole_zero`, whose default is an explicit
`Unsupported` error so a device can never contribute nothing silently. Enabled:
R (literal and model-backed), C, L, K, independent V/I (with the `vsrcpzld.c`
rule), linear E/F/G/H, B sources, S/W switches (the `MODEINITSMSIG` state, as
AC), diode, Gummel-Poon BJT and MOS1 — each delegating to its AC load. Refused:
the XSPICE code-model instances the front end generates for `POLY`/`TABLE`
(C has no pole-zero load for code models and drops them silently), B sources
reading `hertz`, and every device without an implementation. Transmission lines
are not ported at all.

## Output

One plot, `Plotname: Pole-Zero Analysis`, one point, complex vectors named as
C's rawfile writes them — `v(pole(1))`, ..., `v(zero(1))`, ... with type
`voltage` (C's in-memory names are `pole(1)`; `raw_write` prefixes `v(` to
voltage-typed vectors). Poles precede zeros. In a batch run the plot is named
`pzN`, and two `.pz` cards run in reverse deck order (`multi_analysis_pz`).
A `.save` card selecting node vectors cannot be honoured on this plot and fails
the run (C: "no data saved for pole-zero analysis; analysis not run").

## Verification

Golden fixtures (captured once each with `cargo xtask golden capture
--netlist <name>`; compared as unordered root sets under `compare::POLE_ZERO`,
`|Rust - C| <= 1e-6 |C| + 1e-9 max|C root|`): `pz_ladder_cur`,
`pz_bridge_diff`, `pz_transformer`, `pz_cv_loop`, `pz_diode`, `pz_mos1` and the
batch `multi_analysis_pz` (`.ac`, `.op`, two `.pz`). See
[VERIFICATION.md](VERIFICATION.md#m8-pole-zero-analysis-103) for the measured
errors.
