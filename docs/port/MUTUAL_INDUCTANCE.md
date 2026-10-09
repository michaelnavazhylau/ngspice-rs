# K mutual inductance (#80)

**Implemented: K cards coupling two or more inductors for `.op`, `.dc`, `.ac`
and `.tran` (companion trapezoidal/Gear-2 and the explicit `backend=diffsol
method=bdf`), including K cards inside subcircuits, model-backed inductors and
coupled `ic=`/`uic`.** Coupled multiconductor lines (`cpl`) and nonlinear cores
are out of scope.

C references (read-only): `src/spicelib/parser/inp2k.c`, `src/frontend/inpcom.c`
(`inp_compat()`: multi-inductor cards), `src/frontend/subckt.c` (`translate()`),
and `src/spicelib/devices/ind/`: `mutsetup.c` (lookup), `muttemp.c`
(`M = k sqrt(|L1 L2|)`, inductive-system check), `indload.c` (coupled flux),
`mutacld.c` (AC), `indtrunc.c` (truncation error), `mutparam.c` (setters). The
Rust side is `src/netlist/parser/mutual.rs` (grammar),
`src/devices/mutual.rs` (device), the coupling resolution and
definiteness check in `src/devices/circuit.rs`
(`Circuit::mutual_terms`) and the coupled inductor stamps in
`src/devices/rlc.rs`.

## Syntax

```text
Kname L1 L2 [L3 ...] coupling
coupling := value | {expr} | k=value | coefficient=value
```

| Form | Behaviour (as in C) |
| --- | --- |
| `k1 l1 l2 0.98` | positional coefficient (`INP2K` stores it as `coefficient`) |
| `k1 l1 l2 k=0.98`, `coefficient=0.98` | named `MUT_COEFF` setters; `k = 0.98` with spaces is one setter |
| `k1 l1 l2 l3 0.5` | every pair is coupled with the same coefficient (`inp_compat()` writes `k1_1_2`, `k1_1_3`, `k1_2_3`); the port keeps one device |
| negative `k` | allowed; reverses the dot convention |
| inside `.subckt` | inductor names are renamed like instance names (`l1` in `x1` is `l.x1.l1`), so a body K couples only that instance's inductors |
| top-level `k1 l1 l.x1.la 0.3` | names a flattened subcircuit inductor explicitly |
| `{expr}` | evaluated with the scope's `.param`s before elaboration |

The AST keeps the card as written: `inductor1`, `inductor2`, … as
`ParameterKind::Instance` setters followed by one `coefficient`. The writer
emits the coefficient positionally; round trips are `semantic_eq`.

## Equations

`M = k sqrt(|L1 L2|)`, computed from C's `INDinduct` of each inductor (after
temperature coefficients and `scale`, **before** dividing by `m`), while each
inductor still stamps its own `INDinduct / m`. The coupled inductors own their
branch equations:

```text
v+ - v- = d/dt (L i + sum(M i_k))
```

| Analysis | Stamp |
| --- | --- |
| OP/DC | coupled inductors stay shorts; the coupled flux is recorded as the initial transient state (C computes it at `MODEINITTRAN`) |
| AC | `-j w M` at (branch1, branch2) and (branch2, branch1) (`mutacld.c`), assembled as `E` entries of `A + j w E` |
| companion `.tran` | the integrated quantity is the coupled flux `L i + sum(M i_k)` (`indload.c` adds it to `INDflux`); the row gains `-ag0 M` in each coupled branch column (`MUTbr1br2Ptr`); `veq` comes from the coupled-flux history |
| truncation | `INDtrunc` on `INDflux`: the LTE estimate sees the coupled flux |
| `uic` | starting fluxes `L ic + sum(M ic_k)` with unset `ic=` taken as 0 (`indload.c` `MODEUIC`) |
| diffsol BDF | the same off-diagonal `E` entries; the adapter's dense SVD analysis of coupled mass blocks handles them (`k = 1` gives a rank-deficient block that the index-one analysis accepts) |

The K device has no terminals, branches or state and stamps nothing itself;
`Circuit` resolves its names after elaboration and passes each inductor its
`MutualTerm`s (`StampContext::mutual`, `LinearContext::mutual`). Several K
cards on the same pair add up, as C's loads add them.

## Validation and divergences from C

| Input | C | Port |
| --- | --- | --- |
| unknown inductor name | fatal "coupling to non-existent inductor" | positioned parse error with C's wording |
| name of a non-inductor instance (`k1 l1 r1 0.5`) | reads the other device as an inductor (bus error in the reference binary) | error "is not an inductor" |
| the same inductor twice (`k1 l1 l1 0.5`) | warning, zero solution | `Unsupported` |
| no coefficient (`k1 l1 l2`) | silently `k = 0` | parse error |
| a word after the coefficient | reads it as an inductor name or unknown parameter | parse error at the word |
| `|k| > 1`, or an inconsistent set (each `|k| < 1`) | **warning only** ("is not positive definite"), then simulates | `Unsupported`: every connected group of coupled inductors must have a positive **semi**definite inductance matrix (eigenvalues of the unit-diagonal normalized matrix `>= -64 n eps max|lambda|`) |
| all `|k| = 1` | exempt from the warning | accepted when the matrix is semidefinite (ideal transformer); C's exemption also covers indefinite `±1` sets, which the port rejects |
| duplicate K on one pair | warning; loads sum both, check uses the last | summed, check on the sum |
| inductor multiplicity `m != 1` | checks the matrix with `INDinduct` (before `/m`) on the diagonal, so `l1 b 0 lm m=2` (`ind=10m`), `l2 40m`, `k = 0.9` passes silently, then simulates the matrix with `INDinduct / m` on the diagonal, which is indefinite (its transient diverges) | the check runs on the matrix actually stamped (`INDinduct / m` diagonal, `M` from `INDinduct`) and rejects it; the message says the multiplicity, not `|k| > 1`, is the cause and that C does not warn here |

A non-positive-semidefinite inductance matrix stores negative magnetic energy:
its transient grows without bound and its AC answer has no physical meaning,
so the port refuses it in every analysis rather than reproducing C's numbers.
The opt-in test `c_mutual_inductance` shows that C only warns for `k = 1.5`.

## Verification

- C goldens in `cargo xtask golden verify`: `transformer_ac` (1:2 transformer,
  a three-winding K in a subcircuit, a negative coupling; linear AC bound),
  `transformer_tran` (PULSE into a k = 0.99 transformer, trapezoidal) and
  `transformer_ic_uic_tran` (coupled `ic=` free decay, `uic`, Gear-2) and
  `transformer_model_uic_tran` (model-backed coupled inductors with TC and
  `m = 2` at 50 C decaying from instance `ic=`, `uic`, Gear-2), all
  with `compare::TRAN` and worst error 0.000 of the bound. `transformer_tran`
  has no BDF variant: C's backward-Euler restart error after the 1 us pulse
  corner (1 % of `i(l1)` at 2 us against a reltol = 1e-7 reference) exceeds
  even `compare::TRAN_RESTART`, while the BDF result matches that reference to
  1.2e-7 A (`the_bdf_backend_integrates_the_coupled_mass_matrix`).
- `tests/analysis_mutual_inductance.rs`: DC shorts; AC against
  the two-loop closed form for positive, negative, ideal and duplicated
  couplings; a multi-inductor card equals one card per pair; an ideal (k = 1)
  transformer keeps `v(s) = 2 v(p)` to 1e-12 on trap, Gear-2 and BDF; coupled
  RL decays against the matrix exponential from `uic` `ic=` (2.7e-7 A trap,
  1.1e-6 A Gear-2) and from the operating point (2.1e-6 / 7.1e-6 A); explicit
  failures in `.op`/`.ac`/`.tran`.
- `tests/devices_mutual_inductance.rs`: registry entry, name
  resolution, hierarchical names, the definiteness check, model-backed `m=`,
  and the exact DC/companion/linear stamps on the coupled flux.
- `tests/netlist_mutual_inductance.rs` plus the snapshot decks
  `conformance/parser/mutual_inductance.cir` and
  `conformance/cases/error_mutual_coupling.cir`: grammar, positions, errors and
  writer round trips.
- Opt-in live C (`NGSPICE_BIN` absolute): `cargo test -p ngspice-rs --test
  c_mutual_inductance --locked -- --ignored` compares complex AC for duplicate
  K cards, model-backed inductors with `m=`/TC at 50 C and a `±1` three-winding
  system (1e-10 relative), checks C's warning-only behaviour, and checks that
  C stays silent when `m` makes the stamped matrix indefinite.

Model-backed inductors (and capacitors) expose the same truncation slot and
storage element as the scalar devices they delegate to, evaluated at the
analysis temperature: their coupled flux takes part in truncation control and
their instance `ic=` seeds `uic` (golden `transformer_model_uic_tran`).

`.options indverbosity=N` is accepted as a documented no-op: in C it selects
only which stderr diagnostics `muttemp.c` prints, while the port prints none
and always applies the rejection above.

Not covered: K sensitivities (`sens_coeff`) and `.pz`/noise (no such analyses
in the port).
