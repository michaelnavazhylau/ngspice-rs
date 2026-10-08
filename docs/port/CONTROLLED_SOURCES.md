# Linear controlled sources E/F/G/H (#78)

**Implemented: the linear gain forms of E (VCVS), F (CCCS), G (VCCS) and H
(CCVS) for `.op`, `.dc`, `.ac` and `.tran` (companion trapezoidal/Gear-2 and
the explicit `backend=diffsol method=bdf`).** `POLY(n)`, `VALUE=`/`VOL=`/`CUR=`,
`TABLE`, `LAPLACE` and the implicit spice2g6 polynomial are explicit
`SpiceError::NotYetPorted` errors.

C references (read-only): `src/spicelib/parser/inp2e.c` … `inp2h.c`,
`src/frontend/inpcom.c` (`inp_compat()`, `inp_check_syntax()`),
`src/frontend/subckt.c` (`translate()`), `src/spicelib/devices/vcvs/`,
`vccs/`, `cccs/`, `ccvs/` (`*set.c`, `*load.c`, `*par.c`) and
`src/spicelib/analysis/cktfbran.c` (`CKTfndBranch`). The Rust side is
`crates/spice-netlist/src/parser/controlled.rs` (grammar),
`crates/spice-devices/src/controlled.rs` (device and stamps) and the
controlling-branch resolution in `crates/spice-devices/src/circuit.rs`.

## Syntax

```text
Ename n+ n- [vcvs] nc+ nc- gain           voltage gain (V/V)
Gname n+ n- [vccs] nc+ nc- gain [m=..]    transconductance (S)
Fname n+ n- [cccs] vname   gain [m=..]    current gain (A/A)
Hname n+ n- [ccvs] vname   gain           transresistance (ohm)
```

| Form | Behaviour (as in C) |
| --- | --- |
| `gain` as a number or `{expr}` | the leading value; applied **after** any named setter |
| `gain=value` instead of the leading value | a named setter, applied in written order |
| `(in, 0)`, `(in) (0)`, `in,0`, `(vname)` | `(`, `)` and `,` around nodes and the controlling source are skipped (`INPgetNetTok`/`INPgetTok`) |
| HSPICE `vcvs`/`vccs`/`cccs`/`ccvs` keyword | removed only as the fourth whitespace token of a card with exactly 7 (E/G) or 6 (F/H) tokens (`inp_compat`); otherwise it is a node name |
| G/F `m=` tail (`… gain m=2 [gain=…]`) | setters in order; a gain is multiplied by `m` only if `m` was given **before** it (`VCCSparam`/`CCCSparam`) |
| Controlling source of F/H | stored first as `control` (`ParameterKind::Instance`); resolved after elaboration |

So `g1 a b c d 1m m=2` has gain 2 mS while `g1 a b c d gain=1m m=2` has 1 mS;
`f1 a b v1 1 m=2 gain=3` ends at 2. These orders are pinned against
`@g1[gain]` queries of the C binary (`c_reference` test).

Errors:

| Input | Result |
| --- | --- |
| `POLY(n)`, `VALUE=`, `VOL=`, `CUR=`, `TABLE`, `LAPLACE` | `NotYetPorted` (exit 3), naming `inpcom.c`/XSPICE/`asrc` |
| more values after the gain (`e1 o 0 a b 1 2`) | `NotYetPorted`: C turns this into an implicit `POLY(1)` |
| `sens_*` flags, a named `control=` | `NotYetPorted` |
| no gain (`e1 o 0 a b`) | positioned `Parse` error (C: "not enough parameters") |
| `m=` where the gain belongs (`g1 o 0 a b m=2`) | positioned `Parse` error; **C silently builds a zero-gain source** — a deliberate divergence |
| `m=`/`ic=` on E/H | `Parse` error "unknown parameter" (C's `INPdevParse`) |
| a word starting with the HSPICE keyword (`vcvsx`) in the removable position | `NotYetPorted`: C would truncate it to a different node name |

The writer emits `e1 n+ n- nc+ nc- gain` / `f1 n+ n- vname gain` with the
controlling source positional, a final gain positional before its `m=` setters
and a first gain as `gain=`; sequences that cannot be re-parsed to the same
setter order are refused (`write_netlist` round trip and fixed point tested).

## Equations and signs

Output current is positive from `n+` through the source to `n-` (as for
independent sources); a controlling current is the branch current of the named
source, positive from its first terminal through it to its second, so a source
delivering power has a negative current.

| Device | Unknowns | Stamps into `A` (ground rows/columns dropped) |
| --- | --- | --- |
| E | branch `k` | `A[n+][k] += 1`, `A[n-][k] -= 1`, `A[k][n+] += 1`, `A[k][n-] -= 1`, `A[k][nc+] -= mu`, `A[k][nc-] += mu` |
| G | none | `A[n+][nc+] += gm`, `A[n+][nc-] -= gm`, `A[n-][nc+] -= gm`, `A[n-][nc-] += gm` |
| F | none | `A[n+][kc] += beta`, `A[n-][kc] -= beta` |
| H | branch `k` | E's output stamps, then `A[k][kc] -= r` |

The stamps are state independent: the same real entries serve OP, DC sweeps,
AC (no frequency dependence; C's `*acld.c` equals `*load.c`), companion
transient loads and the immutable `E x' + A x = b(t)` assembly used by the BDF
backend. Nothing is added to `E` or `b`. E/H whose outputs share a node are
rejected like `VCVSsetup`/`CCVSsetup` ("instance e1 is a shorted VCVS").

## Controlling sources

`Device::controlling_sources` names the sensed devices and
`Device::findable_branch` marks which devices can be sensed. `Circuit` resolves
the names whenever it numbers the unknowns and hands the rows to the device as
`StampContext::controls`/`LinearContext::controls`; nothing is cached across a
renumbering. As in `CKTfndBranch`:

- independent voltage sources and E/H can be sensed (also when they follow the
  F/H in the deck); inductors, resistors, current sources, G and F cannot;
- an unknown name or a non-findable device is a positioned `Parse` error at the
  reference ("f1: unknown controlling source vx", "… has no findable branch
  current"), reported by `Circuit::finalize` before any analysis;
- inside a subcircuit the reference is renamed like an instance name
  (`translate_inst_name`): `vs` in `x1` becomes `v.x1.vs`. A body therefore
  cannot sense a top-level source by its bare name, exactly as in C.

## Output

E and H add a branch-current vector named like a voltage source's:
`i(e1)`, `i(h.x1.h1)` (C writes `e1#branch`). `.save`, `.print`, `.measure`
and `.four` accept `i(<e…>)`/`i(<h…>)`; G and F have no branch and `i(g1)`
remains `NotYetPorted`. Controlled sources are DC-sweep context, not sweep
targets.

## Verification

- Netlist: `crates/spice-netlist/tests/controlled_sources.rs` (forms, positions,
  errors, writer round trip), `conformance/parser/controlled_sources.cir` and
  `conformance/cases/error_controlled_{poly,gain}.cir` snapshots, and the opt-in
  live C setter-order probe `parsed_controlled_source_setters_match_live_c_gains`.
- Devices: `crates/spice-devices/tests/controlled_sources.rs` (C stamp entries,
  signs, mode independence, forward/E/H controls, hierarchical controls,
  errors, registry).
- Analyses: `crates/spice-analysis/tests/controlled_sources.rs` (analytic OP
  signs, an ideal op-amp (gain 1e6) in inverting, non-inverting and follower
  loops, DC sweeps, frequency-independent AC, analytic second-order transient
  on both backends, `.save` and `.measure` of branch currents, singular ideal
  loops).
- C goldens: `controlled_op` (all four devices, an F controlled by an E branch,
  an H inside a subcircuit, the HSPICE keyword and parentheses), `controlled_ac`
  (finite-gain integrator, G into an RC, F/H sensing a load current) and
  `controlled_tran` (PULSE, both backends; BDF under the peak-scaled
  `TRAN_RESTART` bound like `rl_pulse_tran`), all in `cargo xtask golden verify`
  with the existing linear tolerances.

## Limits

- Nonlinear forms (POLY/VALUE/TABLE/LAPLACE) are not ported.
- AC solves have no equilibration. A resistive op-amp loop with gain `1e6`
  solves in `.op`, `.ac` and both transient backends, but the
  `controlled_ac` integrator (1 uF feedback, G into an RC load) with open-loop
  gain `1e5` trips the sparse LU conditioning diagnostic at 100 Hz ("numeric
  rank is unresolved at this conditioning; rescale the system") instead of
  returning a result; the committed deck uses gain `1e4`. The OP fixture also
  uses `1e4` because with `1e6` C's own rounding (about 8e-12 relative,
  against an exact Rust result) exceeds the 1e-12 DC comparison bound.
- A `.ic` on a node driven by an E/H output is not reduced against the E/H
  relation (only V sources and inductors are), so the constrained initial bias
  is singular and fails explicitly (C runs such a deck through its
  `cktload.c` `.ic` row handling).
- `m=` scaling and `sens_*` sensitivity setters are limited to the forms above.
