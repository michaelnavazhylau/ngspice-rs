# Uniform distributed RC lines (URC, #85 part 1)

**Implemented:** `U` instances with `urc` models, expanded at elaboration into
the lumped R/C (or R/diode) ladder that `urcsetup.c` builds, with ngspice's
names for every generated node and element. OP, DC, AC, companion transient
and noise therefore see the line exactly as C does. `.pz` is refused (C
aborts it) and `.sens` is `NotYetPorted`; see below.

C references: `src/spicelib/parser/inp2u.c`, `src/spicelib/devices/urc/`
(`urc.c` parameter tables, `urcmpar.c`, `urcparam.c`, `urcsetup.c`).
Rust: `src/netlist/parser/urc.rs` (grammar), `src/devices/urc.rs`
(schema and expansion), `Circuit::add_instances` (commit).

## Syntax

```spice
Uname n1 n2 nref model [l=len] [n=lumps]
.model model urc [urc] [k=…] [fmax=…] [rperl=…] [cperl=…] [isperl=…] [rsperl=…]
```

- `n1`/`n2` are the line ends, `nref` the capacitive reference (`URCnames`:
  P1, P2, Ref). The model is required (C: "Unable to find definition of
  model").
- Instance setters `l` (`IF_REAL`) and `n` (`IF_INTEGER`, rounded
  `floor(x + 0.5)` as `inpgval.c`), `=` optional, last setter wins, literals
  or `{expr}`. Any other setter is an error (C: "unknown parameter").
- Model setters and defaults (`urcsetup.c`): `K` 1.5, `FMAX` 1 GHz,
  `RPERL` 1000 ohm/m, `CPERL` 1 pF/m, `RSPERL` 0; `ISPERL` has no default and
  selects the diode ladder when given. The bare `urc` flag is a no-op
  (`URC_MOD_URC`). Unknown setters and `level` are refused (C warns and
  ignores them).

## Expansion design

C's URC device has no load, AC or pole-zero routine: `URCsetup` creates
ordinary resistors, capacitors and diodes and the line no longer exists as a
device. The port mirrors that with a **factory expansion into existing
devices** rather than a composite device:

- the generated elements are the already-verified `Resistor`, `Capacitor`
  and `Diode`, so no URC-specific stamping, state, noise, sensitivity or
  pole-zero hook is needed, and every analysis matches C structurally;
- names and plots are C's: generated elements are top-level devices named
  `u1#rlo1`, `u1#chi2`, `u1#dlo3`, … (observable as `@u1#rlo1[resistance]`)
  and the internal nodes are ordinary nodes `u1#hi1`, `u1#lo1`, … that C's
  default save set keeps, so rawfiles carry `v(u1#hi1)` as ngspice writes
  them. Inside subcircuits the usual renaming gives `u.x1.u1#hi1`.

`Circuit::add_instances` calls `devices::urc::expand` for each `U` instance
and commits its elements with the rest of the batch (atomic: a failure leaves
nodes, devices and branch rows unchanged). As in C, where the URC instance
stays in the circuit next to what `URCsetup` generated, the instance itself
is kept as a load-free, terminal-less `UrcLine` device named `u1`:

- it answers `urcask.c`'s `@u1[l]` and `@u1[n]` (the computed section count);
  the node-number asks `pos_node`/`neg_node`/`gnd` are explicit gaps;
- it is noiseless and linear in `.disto` (`DEVnoise`/`DEVdisto` are `NULL`);
- `.pz` fails explicitly: C's `DEVpzSetup` is `URCsetup`, which tries to
  create the generated elements again and aborts the run ("device already
  exists"), so the port refuses rather than report poles C never computes;
- `.sens` is `NotYetPorted`: C's `sgen` also lists the URC's own parameters
  (`u1:k`, `u1:rperl`, `u1:isperl`, `u1:rsperl`, `u1_l`, all zero), which
  the port's sensitivity records do not model yet.

### Names and order (`urcsetup.c`)

For section `i = 1..=lumps`: node `#hi<i>`, then `#lo<i>` (not in the last
section, whose `lo` side is its `hi` node); resistor `#rlo<i>` (`lo` chain from
`n1`) and `#rhi<i>` (`hi` chain back to `n2`); then capacitor `#clo<i>` and,
except in the last section, `#chi<i>` to the reference, or diodes
`#dlo<i>`/`#dhi<i>` (anode on the line, cathode on the reference) of the
generated model `<name>#diodemod`.

### Values

`p = K`, `r0 = l*RPERL`, `c0 = l*CPERL`, `i0 = l*ISPERL`:

- `lumps = n`, or `max(3, trunc(ln(wnorm*((p-1)/p)^2)/ln p))` with
  `wnorm = FMAX*r0*c0*2*pi`, and 3 whenever `wnorm < 35`;
- `r1 = r0(p-1)/(2p^lumps - 2)`, `c1 = c0(p-1)/(p^(lumps-1)(p+1) - 2)`, `i1`
  likewise, `rd = l*lumps*RSPERL`;
- section `i`: `r = p^(i-1) r1` on both chains, `c = p^(i-1) c1`; diodes use
  `cjo = c1`, `is = i1`, `rs = rd` and `area = p^(i-1)`.

The arithmetic follows C operation by operation (`pow`/`log`, accumulated
`prop *= p`), so element values agree with C's asks bit for bit when the
deck's literals parse to the same doubles (see below).

## Deliberate divergences (explicit errors)

C accepts these and then simulates something degenerate; the port refuses:

| Input | C behaviour | Port |
| --- | --- | --- |
| missing or nonpositive `l` | length 0: zero capacitors, resistors clamped to `RESMIN` | error |
| `n < 1` | no sections: terminals left unconnected | error |
| `K == 1`, `K <= 0` | 0/0 element values (NaN solution) | error |
| `RPERL <= 0`, `ISPERL <= 0`, `CPERL == 0` without `ISPERL` | zero/negative elements | error |
| more than 10 000 sections | builds them | error (resource bound) |
| `level`, unknown model setters | warning, ignored | error |
| a deck node named like a generated one (`u1#hi1`) | `CKTmkVolt` fails (`E_EXISTS`) | error |

`RSPERL` without `ISPERL` is accepted with no effect, as in C. The diodes'
series-resistance nodes are device-internal (`#anode`, C `#internal`) and,
as in C, not part of the compared save set.

C's `INPevaluate` reads some decimals one ulp away from the nearest double
(`1.2` is `12*0.1 = 1.2000000000000002`), a parser-wide divergence already
recorded in TODO.md. A ratio such as `k=1.2` therefore moves `p^lumps` and
every element value by about 1e-15 relative; it is far below every
simulation tolerance.

## Verification

- C goldens (captured individually, reviewed in `tests/golden_rawfiles.rs`):
  `m10_urc_ac` (FMAX rule, `n=6` with a bootstrapped reference, `n=1`;
  `compare::AC`), `m10_urc_tran` (K=2 FMAX rule, `n=4` on a biased reference,
  C's 3-section minimum; `compare::TRAN`) and `m10_urc_diode_tran` (ISPERL
  ladder with RSPERL, `.options reltol=1e-6` so both integrators' truncation
  error in the small source currents sits well inside `compare::TRAN`; at the
  default RELTOL the zero-crossing currents exceed the bound up to 5.5x, at
  1e-5 the worst error is 0.64 of the bound and at 1e-6 0.12: it shrinks with
  RELTOL, i.e. it is integration error, not model error). The two linear
  decks need no tightening (AC at 1e-10; transient well inside `TRAN`).
- `tests/urc_lines.rs`: grammar, names/order/topology, exact element values,
  rounding, diode ladder, subcircuits, explicit errors and atomic failure.
- Opt-in `tests/c_urc_reference.rs`: decks beyond the goldens at an operating
  point (rounded `n`, `K < 1`, a 24-section line, subcircuit, diode ladders),
  every vector by name plus C's `@…[resistance|capacitance|area]` asks of the
  generated elements and `@u1[l]`/`@u1[n]` to 1e-15.
- `.noise` of a URC deck was checked by hand against C (same spectrum and
  totals to the printed digits); it has no committed golden.

## Not covered

- `.sens` (above); `.dc @u1[l]` sweeps (refused: the instance exposes no
  sweepable parameter, and C would not re-run `URCsetup` either); the diffsol
  BDF backend is untested with URC decks (the generated devices apply their
  own backend rules).
