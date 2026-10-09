# Behavioural sources (#79)

**Implemented: B arbitrary sources (`v=`/`i=` expressions) and the nonlinear
E/G/F/H forms `VALUE=`/`VOL=`/`CUR=`, `TABLE`, `POLY(n)` and the implicit
spice2g6 `POLY(1)`, for `.op`, `.dc`, `.ac` and the companion `.tran`
(trapezoidal/Gear-2).** `LAPLACE`, `ddt()` and the statistical functions
(`agauss`, `gauss`, `aunif`, `unif`, `limit`) are explicit `NotYetPorted`
errors; the diffsol BDF backend rejects behavioural sources
like every nonlinear device.

C references (read-only behaviour): `src/spicelib/parser/inp2b.c`,
`inpptree.c`, `inpptree-parser.y`, `ptfuncs.c`, `ifeval.c`;
`src/spicelib/devices/asrc/` (`asrcset.c`, `asrcload.c`, `asrcacld.c`,
`asrcpar.c`, `asrctemp.c`, `asrcfbr.c`); `src/frontend/inpcom.c`
(`inp_meas_current`, `inp_compat`, `inp_poly_2g6_compat`,
`inp_bsource_compat`, `inp_modify_exp`); `src/xspice/enh/enhtrans.c`;
`src/xspice/icm/analog/pwl/cfunc.mod`, `src/xspice/icm/spice2poly/`.

| Rust | Job |
| --- | --- |
| `spice_netlist::bexpr` | expression syntax tree (separate from numparam's [`expr`](PARAM_EXPRESSIONS.md): it references circuit quantities) |
| `spice-netlist` `parser/bexpression.rs`, `parser/behavioural.rs` | winnow grammar over the raw card text; B card and nonlinear E/G/F/H forms |
| `spice_netlist::behavioural` | `inpcom.c` lowering passes and `.param`/`.func`/numparam resolution |
| `spice_devices::behavioural` | the device: compiled value/derivative evaluator, stamps, XSPICE `pwl` map |

## Syntax

```text
Bname n+ n- v=expr [m= tc1= tc2= temp= dtemp= reciproctc= reciprocm=]
Bname n+ n- i=expr [same setters]
Ename n+ n- value={expr} | vol={expr} [B setters]
Gname n+ n- value={expr} | cur={expr} [m=k]
Ename n+ n- table {expr} [=] (x0, y0) (x1, y1) ...
Gname n+ n- table {expr} [=] (x0, y0) (x1, y1) ... [m=k]
Ename n+ n- nc+ nc- table=(x0, y0, x1, y1, ...)          LTspice form
Ename n+ n- poly(n) nc1+ nc1- ... ncn+ ncn- c0 c1 c2 ...
Gname n+ n- poly(n) nc1+ nc1- ... c0 c1 ... [m=k]
Fname n+ n- poly(n) v1 ... vn c0 c1 ... [m=k]
Hname n+ n- poly(n) v1 ... vn c0 c1 ...
Ename n+ n- nc+ nc- c0 c1 c2 ...                         implicit POLY(1)
```

The expression grammar follows the bison grammar, lowest precedence first:
`?:` (right associative), `||`, `&&`, `==`/`!=`/`<>`, `<`/`>`/`<=`/`>=`,
`+`/`-`, `*`/`/`, unary `-`/`+`/`!`, `^`/`**` (left associative, so `2^3^2`
is 64; `-2^2` is -4; `2^-1^2` is `2^(-(1^2))`). Operands: numbers with SPICE
scale factors (unit letters swallowed), `v(node)`, `v(n1, n2)`, `i(source)`,
`time`, `temper`, `hertz`, `pi`, `e`, `.param` names and function calls.
Comparisons and logic become C's `eq0`/`ne0`/`gt0`/`lt0`/`ge0`/`le0` of a
difference (`a && b` is `eq0(eq0(a) + eq0(b))`).

As after `inp_modify_exp()`:

- braces and single quotes are **whitespace**, not grouping: `v=2*{1+1}` is
  3 (verified against C);
- numeric literals are rounded to 11 significant digits (`%18.10e`);
  `.param` values substitute with full precision;
- the expression ends at the first token that cannot continue it; the rest
  of the card must be `name=value` setters (`v=2 3` is an error, as in C).

A card whose text (after `inp_remove_ws()`) contains `=pwl(` is left alone by
`inp_bsource_compat()`: its `{...}` groups are numparam values, literals keep
full precision and bare `.param` names are an error (C's parse-tree check
fails on them).

`.func` calls are expanded like `inp_expand_macro_in_str()`: the body replaces
the call with each formal bound to its parenthesised argument (`f(1+1)` with
body `x*2` is 4), free names resolve at the call site, a `.func` may redefine a
built-in function, and `v(name)`/`i(name)` inside a body name the node/source
literally (C does not substitute formals there; verified).

## Functions

The complete `inpptree.c` table: `abs acos acosh asin asinh atan atanh cos
cosh exp ln log log10 sgn sin sinh sqrt tan tanh u uramp ceil floor nint u2
pwl eq0 ne0 gt0 lt0 ge0 le0 pow pwr min max ternary_fcn`. Unknown names are a
parse error ("no such function"); `ddt` is `NotYetPorted`, and so are
`agauss`, `gauss`, `aunif`, `unif` and `limit`, which C replaces by one drawn
value before parsing a B line (`src/frontend/inp.c` `eval_agauss()`, the
`inp_fix_agauss_in_param()` set of `inpcom.c`; a user `.func` of the same name
is expanded first, as in C);
`pwl_derivative` (internal to `pwl`, C would dereference a missing table) is
`Unsupported`.

Value rules (`ptfuncs.c`, no compatibility mode, as the reference binary runs):

| Function | Rule |
| --- | --- |
| `/` | divisor moved `gmin * 1e-20` away from zero (`1/0` = `1e32`) |
| `^` | `pow(a, b)` for `a >= 0`; `pow(a, round(b))` for `a < 0` and an integer `b` (10 ulps); else 0 |
| `pow(a, b)` | as `^` but `pow(0, b)` = 0 |
| `pwr(a, b)` | `sign(a) |a|^b` |
| `exp` | `1e99` above 227.9559242 |
| `log`, `ln`, `log10`, `sqrt` | negative argument is an error; `log(0)` = `-1e99` |
| `sin`, `cos`, `tan` | argument reduced by `x - (int)(x/2pi)*2pi` (`pi` for `tan`) first |
| `u` | 0, 0.5 at 0, 1; `uramp` = `max(x, 0)`; `u2` clamps to [0, 1] |
| `nint` | round half to even |
| `pwl(x, x0, y0, ...)` | literal points (a number or `-number`), monotonic abscissa, linear extrapolation beyond the ends |
| `min`, `max`, `?:` | `?:` selects by `condition != 0` |

## Derivatives

Newton and AC need `d f / d x` for every quantity `x` the expression reads.
ngspice differentiates the tree symbolically once (`PTdifferentiate`) and
evaluates the derivative trees with the same C functions. The port evaluates
value and gradient together (forward mode) and applies **C's derivative rule
at every node**, so the Jacobian is C's Jacobian, including its deliberate and
accidental quirks (all verified against the C binary's AC analysis):

- `u`, `sgn`, `floor`, `ceil`, `nint` and the comparisons have derivative 0;
  `min`/`max` take the derivative of the selected operand (`lt0(a-b) ? a' : b'`);
  the ternary differentiates the selected branch;
- `a^b` with a constant `b` differentiates as `b pwr(a, b-1) a'`, i.e. as
  `|a|^b`: for an odd integer `b` and `a < 0` the derivative has the wrong
  sign (`v(in)**3` at -0.45 V gives -0.6075 in C and in the port);
- `pwr(a, b)` with a constant `b` uses `b pow(a, b-1) a'`, which is 0 for a
  negative `a` and a non-integer `b`;
- `pwl` on a descending table takes the slope C's ascending-only search
  finds (`PTpwl_derivative`);
- divisions in derivative trees include the `gmin * 1e-20` fudge.

Elsewhere the analytic gradient matches finite differences for every function
and operator (`spice-devices/tests/behavioural_sources.rs`). Where C would
continue with a NaN or an infinity (`acos(2)`, `cosh(1000)`), the port stops
with a numerical error naming the function and the non-finite value or
derivative. This includes infinite *slopes* at an intermediate Newton iterate:
`v(in)^0.5` or `v(in)^-1` at the 0 V start has derivative `b pwr(0, b-1) =
inf`; C loads it, carries a NaN iterate for the unknowns that depend on that
row and converges on the next iteration, while the port stops ("non-finite
derivative inf (value 0) in '^'"). See Limits.

### Singular slopes at the zero start

C's `PTdivide` fudge gives `1/v(x)`, `sqrt(v(x))`, `log(v(x))` and divisions a
slope of about `1e32` at a 0 V iterate (and `log(0)` is `-1e99`). The first
linearisation is well posed, but row scaling alone leaves the output column
`1e32` times weaker than its coupling and trips the sparse LU conditioning
guard. Newton (`spice-analysis/src/newton.rs` `linearised_solve`) therefore
retries a numerically failed row-equilibrated solve once with Curtis-Reid
power-of-two row/column balancing and up to three rounds of iterative
refinement (`EquilibratedSparseLu::new_balanced`/`solve_refined`); the
rank/conditioning guard still applies to the balanced matrix and every
solution is checked against the original equations. Solves the first path
accepts are unchanged, and when both fail the original error is reported.
`bsource_zero_op`, `bsource_zero_dc` and `bsource_zero_tran` are C goldens
for this.

## Device equations

With `k = m (1 + tc1 d + tc2 d^2)`, `d = T + dtemp - 300.15 K` (`T` the
instance `temp=` or the circuit temperature; `reciproctc=1` inverts the
temperature factor, `reciprocm=1` divides by `m`), each load linearises at the
present iterate `x0` like `ASRCload`:

| Output | Stamps |
| --- | --- |
| `v=` (branch `r`) | `A[n+][r] += 1`, `A[n-][r] -= 1`, `A[r][n+] += 1`, `A[r][n-] -= 1`, `A[r][x_i] -= k g_i`, `b[r] += k (f - g.x0)` |
| `i=` | `A[n+][x_i] += k g_i`, `A[n-][x_i] -= k g_i`, `b[n+] -= k (f - g.x0)`, `b[n-] += k (f - g.x0)` |

So `v(n+) - v(n-) = k f` and a current `k f` flows from `n+` through the
source to `n-` (V/I conventions; a delivering voltage B source has a negative
`i(b1)`). AC stamps the same Jacobian at the bias point without RHS
(`ASRCacLoad`), real and frequency independent. `time` is 0 in OP/DC and the AC
linearisation, the transient time otherwise; `temper` is the circuit
temperature in Celsius (it follows `.dc temp` sweeps); `hertz` is the AC
frequency and 0 elsewhere (also in an `.op` after an `.ac` of the same deck,
as observed with the C binary). As `acan.c` does when `CKTvarHertz` is set,
an AC analysis of a circuit with a `hertz`-dependent device re-solves the
operating point at every frequency (warm-started from the previous one) and
linearises there.

**Breakpoints:** ngspice registers none for B sources, and neither does the
port; a time function (including `pwl(time, ...)` corners and `u(time-t0)`) is
followed only by the truncation-error step control of the storage elements.
Use a maximum step (`.tran tstep tstop 0 tmax`) to resolve fast time functions;
`bsource_tran` does, because otherwise both simulators carry percent-level
errors of their own.

**Newton limiting:** `asrcload.c` applies no limiting. The port's Newton uses a
global 0.2 V voltage-step damping in place of junction limiting; behavioural
devices opt out (`Device::limits_voltage_steps`), and when any device opts out
the damping watches only the nodes of the junction devices. A 1000x B gain on
5 V therefore converges in one step instead of thousands. Circuits without
behavioural sources keep the previous policy exactly.

Not modelled: C scales B outputs by the source-stepping factor only in the
transient operating point (`MODETRANOP`), and `ASRCconvTest` adds a
function-value convergence test; both change the iteration path, not the
converged solution.

## Front-end lowering (`inpcom.c`)

`spice_netlist::behavioural::lower_nonlinear_sources` runs before `.param`
literalization and subcircuit expansion, like C's front end, so generated
names are renamed per instance (`x1.e1_int1`, `b.x1.be1`, `i(v.x1.v_b1)`):

| Form | Becomes |
| --- | --- |
| `e1 n+ n- value={f}` | `e1 n+ n- e1_int1 0 1` and `be1 e1_int1 0 v=f` (B setters move to `be1`) |
| `g1 n+ n- value={f} m=k` | `g1 n+ n- g1_int1 0 k` and `bg1 g1_int1 0 v=f` |
| `e1 n+ n- table {f} = pairs` | `e1 n+ n- e1_int1 0 1`, `be1 e1_int2 0 v=f`, XSPICE `pwl` instance `ae1` from `v(e1_int2)` to `e1_int1` |
| `g1 ... table ... m=k` | the same with gain `k`; `[`, `]`, `%` in G names become `_` |
| `e1 n+ n- nc+ nc- table=(...)` | `e1 n+ n- e1_int1 0 1`, `ae1` reading `v(nc+, nc-)` |
| one TABLE pair `(x0, y0)` | `v<name> <name>_int1 0 y0` instead of the B/XSPICE pair |
| `POLY(n)` on E/G/F/H (and the implicit `POLY(1)`) | the `spice2poly` instance `a$poly$<name>` with output `v=` (E/H, branch `i(a$poly$e1)`) or `i=` (G/F) |

The XSPICE instances are internal designator-`a` behavioural devices (user
`a` cards stay unported): `ae1` evaluates the `pwl` code model's static map
(`input_domain` 0.1, or 0.001 for the LTspice form, `fraction=TRUE`,
`limit=TRUE`: flat outside the table, parabolic corners over 10 % of the
shorter adjacent segment); the model's iteration-to-iteration input limiting is
a Newton aid and is not reproduced. `a$poly$...` is the SPICE2 polynomial
`c0 + c1 x1 + ... + cn xn + c(n+1) x1^2 + c(n+2) x1 x2 + ...`, terms in
`nxtpwr()` order (total degree, then lexicographically descending exponents),
`m=` multiplying G/F outputs; `m=` on an E/H POLY is rejected (C drops it with
a warning).

`inp_meas_current()` is reproduced too: an `i(name)` in a behavioural
expression whose `name` does not start with `v` and is not a simple linear
E/H senses the current through an inserted zero-volt source `v_name`; `name`'s
first node becomes `<node>_vmeas_<n>` (`n` counting references in deck order)
and the reference becomes `i(v_name)`. The device is searched in the same
scope only. As in C, an `i(` preceded by a comma (`max(0,i(b2))`) is not
rewritten and must name a findable branch (V, E, H or voltage B).

## Errors

| Input | Result |
| --- | --- |
| malformed expression, trailing text, two `v=`/`i=`, missing `v=`/`i=` | positioned `Parse` error |
| undefined `.param`, unknown function, wrong argument count, non-literal or non-monotonic `pwl()` points | `Parse` error at the name |
| `ddt()`, `LAPLACE` | `NotYetPorted` |
| `agauss()`, `gauss()`, `aunif()`, `unif()`, `limit()` | `NotYetPorted` at the call (`src/frontend/inp.c` `eval_agauss`; `inpcom.c` `inp_fix_agauss_in_param`) |
| `v=` with coinciding nodes | `Unsupported` ("shorted ASRC") |
| `temp=` and `dtemp=` together | `Unsupported` (C ignores `dtemp` with a message) |
| domain error or non-finite value/derivative during a load | `Numerical`, naming the function and the offending value or derivative |
| `m=` on an E/H POLY, a four-node G TABLE, `VALUE=` on F/H | parse/unsupported errors |
| a nonlinear E/G form handed to the single-card registry factory | `Unsupported` (the lowering needs the whole deck) |

## Verification

- C goldens in `cargo xtask golden verify`, captured one at a time:
  `bsource_op`, `bsource_dc`, `bsource_ac` (nonlinear B sources, 1 ppm
  `NONLINEAR` bound), `bsource_tran` (`compare::TRAN`), `evalue_op` (VALUE
  forms, a subcircuit, inserted current sensing), `gtable_dc` (TABLE forms,
  needs the `analog` code models), `epoly_dc` (POLY forms, needs
  `spice2poly`) and `bsource_zero_op`/`bsource_zero_dc`/`bsource_zero_tran`
  (sqrt/log/reciprocal/division started from the 0 V Newton iterate). Fixtures that need XSPICE code models name them on a
  `* xtask-codemodels:` comment; the capture then writes a scratch
  `.spiceinit` loading only those libraries (see VERIFICATION.md).
- `crates/spice-devices/tests/behavioural_sources.rs`: finite-difference
  checks of every function's derivative, C value/derivative quirks, Newton
  residual/Jacobian consistency of whole loads, `ASRCload`/`ASRCacLoad`
  stamps, the temperature factor and errors.
- `crates/spice-netlist/tests/behavioural_sources.rs`: grammar, positions,
  writer round trips, resolution and the lowering passes;
  `conformance/parser/behavioural_sources.cir` and the
  `error_behavioural_*`/`error_controlled_poly` cases are snapshotted.
- `crates/spice-analysis/tests/behavioural_sources.rs`: analytic OP/DC/AC/
  transient results, `temper` sweeps, `hertz` in AC, undamped large steps,
  zero-start singular slopes in every analysis and the infinite-slope error.
- `crates/spice-maths/tests/equilibration.rs`: Curtis-Reid balancing and
  refinement on the zero-start linearisation; singular systems stay rejected.
- Opt-in: `NGSPICE_BIN=/abs/ngspice cargo test -p spice-analysis --test
  c_behavioural_reference -- --ignored` compares 47 expressions' values (OP)
  and derivatives (one-point AC) with the C binary at four bias points to
  `1e-12` relative.

## Limits

- An infinite derivative at an intermediate Newton iterate stops the port
  where C continues through a NaN iterate and converges: `v(x)^b` with a
  constant `b < 1` (`v(in)^0.5`, `v(in)^-1`, `v(in)**-2`) or `pow(0, -1)`
  whose controlling node starts at 0 V. This is a C-parity gap for common
  decks; write `sqrt(v(x))` or `1/v(x)` (finite `1e32` slopes) instead.
- `ddt()`, the statistical functions (`agauss`, `gauss`, `aunif`, `unif`,
  `limit`), `LAPLACE` (XSPICE `s_xfer`), the PSPICE/HSPICE/
  LTspice compatibility variants of `^`, `exp` and `pwr`, and user XSPICE `a`
  cards are not ported.
- `.func` bodies are expanded after the lowering passes, so an `i()` that only
  appears inside a `.func` body is not rewritten by `inp_meas_current()`;
  numparam-only constructs inside `.func` bodies (`?:`, comparisons) remain
  unsupported by the numparam grammar.
- An E/H written with a named `gain=` counts as a simple linear source here;
  C inserts a measurement source for it (`=` on the card), which changes only
  vector names.
- The diffsol BDF backend rejects behavioural sources; use the default
  companion driver.
