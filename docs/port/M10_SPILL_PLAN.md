# M10 spill: TXL, MESA, HFET1 and HFET2 — slice plan

Planning document, 2026-10-15, against `main` = `d806efe` (M10 waves 0–3
merged). Nothing here is implemented yet: `y` is `pending` in the registry and
`z` is `bounded` with scope "MESFET level 1 (Statz, `mesload.c`)"
(`src/devices/factory.rs` `ELABORATED`). This replaces the two-line
"## Spill" stub in [M10.md](M10.md#spill-bsim-and-remaining-lines-proposed-m11)
for the `#85`/`#83` remainder; BSIM3/BSIM4 (#90) keeps its own entry.

## Why this is planned rather than estimated

The C references were read (`src/spicelib/parser/{inp2y,inp2z}.c`,
`src/spicelib/devices/{txl,mesa,hfet1,hfet2}/`) and probed with the local
reference binary
(`/Users/michaelnavazhylau/Code/electronics-work/spice-port/ngspice_test/build/src/ngspice`).
Two findings change the shape of the work and are the reason TX0 below exists:

1. **C's TXL has no AC load.** There is no `txlacld.c`; `.ac` runs
   `TXLload`, which takes its frequency argument from `ckt->CKTdelta`, and
   nothing in the AC path assigns `CKTdelta` (verified: no mention in
   `niaciter.c`, `acan.c`; `cktdojob.c` leaves it 0). Measured on
   `Y1 in 0 out 0 ymod` with `V1 ac 1` and a 7.4 fF far-end load:

   | Model | Output | Expected if the line worked |
   | --- | --- | --- |
   | `r=12.45 l=8.972n c=0.468p length=16` (Z0 = 138.5) | `vm(out)` = 1.3846e-11 at 1 MHz, 1.3846e-11 at 1 GHz | ≈ 1 |
   | `r=1 l=250n c=100p length=1` (Z0 = 50) into 50 Ω | `vm(out)` = 2.5e-12, frequency-independent | ≈ 0.5 |

   The magnitude is numerical residue of a near-singular matrix, not a
   response: it does not vary with frequency.

2. **C's TXL transient only works when the bias is settled before the
   excitation.** The same `r=1 l=250n c=100p length=1` line (TD = 5 ns) into a
   50 Ω load, driven by a 1 V step:

   | Step at | `v(out)` | `i(V1)` |
   | --- | --- | --- |
   | 1 ns | −2.5e-12 for the whole run, past 6 ns | −20.4 mA (input port = 1/Z0 to ground) |
   | 15.9 ns | 0 → 0.99 in ≈ 5 ns | −20.0 mA → −19.8 mA |

   In the first case `right_consts_txl` never returns a usable history
   interval, so the port-2 coupling terms (`TXLibr1NegPtr`, `TXLibr2PosPtr`,
   `TXLibr1Ibr2Ptr`, `TXLibr2Ibr1Ptr`) stay zero and the two ports are
   decoupled; the same deck with a late step propagates normally. The C
   regression decks (`tests/transmission/txl{1_1,2_3}_line.cir`) all hold the
   bias for 15.9 ns before pulsing, so they sit in the working regime — and
   the local C binary reproduces `txl2_3_line.out` to ~1e-6 relative (first
   differing row: index 17, 4.999865 vs 4.999866).

`.op`/`.dc` are well defined in both regimes: the DC stamp is the series
resistance `R * length` with `0.1 * gmin` on the four node entries
(`TXLload` `MODEDC`), i.e. `v(p) − v(n) = R len i1`. Measured for the 1 Ω line
into 50 Ω: `v(out)` = 9.803922e-01, `y1#branch1` = +1.960784e-02,
`y1#branch2` = −1.960784e-02.

Nothing below is a commitment to full parity: the port's rule stands, that
unsupported cases fail with an explicit error naming the C reference.

## Orders of magnitude

| C source | Lines | Port target |
| --- | --- | --- |
| `parser/inp2y.c` | 257 | `src/netlist/parser/txl.rs` |
| `devices/txl/` (`txlsetup.c` 1199 + `txlload.c` 683 + the rest) | 2402 | `src/devices/txl.rs`, `src/devices/txl/` |
| `devices/mesa/` | 3002 | shared HFET shell + `mesa1/2/3` channels |
| `devices/hfet1/` | 2741 | shared HFET shell + HFET1 channel |
| `devices/hfet2/` | 1841 | shared HFET shell + HFET2 channel |

## Part 1 — TXL (`y`, #85 remainder)

### TX0 · Characterisation and policy (no production code)

*Goal.* Turn the probes above into a checked-in record and settle the two
policy questions, before any of `txlsetup.c`'s 1199 lines are ported.

- Add `docs/port/TXL.md`: card, model/instance parameters, the `ReadTxL` case
  split (`R/L < 5e5` with `G < 1e-2` selects the "lossless" branch
  `h3_aten = sqrt(C/L)`, `h2_aten = 1`, `taul = sqrt(C L) l`; otherwise
  `g = 1/(R l)` and `main_pade`), the DC stamp, the transient stamp, the
  history/step bound ("your time step is too large for TXL tau" →
  `CKTmaxStep = 0.9 * taul`), and the AC defect with the measured numbers.
- Capture the probes as fixtures: the C regression decks
  (`tests/transmission/txl1_1_line.cir`, `txl2_3_line.cir`) and a late-step
  deck of our own, both in `conformance/netlists/` with C goldens.
- Decide: **AC policy** (recommend: refuse `y` in `.ac`/`.sp`, as the port
  already refuses an RG LTRA line in AC, because C's answer is
  frequency-independent numerical residue) and **transient policy**
  (recommend: port it, but only with the settled-history regime covered by
  goldens, and with the decoupled regime documented as C's behaviour rather
  than implemented as a separate mode).

*Exit.* The doc exists, the three decks are committed with C goldens, and the
two policies are recorded. No device code yet.

### TX1 · Front end (`y`)

*Goal.* Parse the card without creating a device, and refuse what C cannot
support.

- Grammar in `src/netlist/parser/txl.rs` after `src/netlist/parser/ltra.rs`:
  `Yname n1 g1 n2 g2 model [len=<v>]` — four terminals, then the model token
  (`inp2y.c` reads them in that order), then the `len` instance token.
- Model schema `txl` in `src/devices/schema.rs` / `models.rs`:
  `r`, `l`, `c`, `g`, `length` (all per length except `length`) and the
  `txl` flag. **All five are required** (`TXLsetup` reports "lossy line
  series resistance not given", "… parallel conductance not given", "…
  series inductance not given", "… parallel capacitance not given", "…
  length must be given"); this is stricter than LTRA's defaults and is C's
  actual behaviour, not a choice.
- A model whose type is `CplLines` sends C into `INP2P`; CPL is out of scope,
  so that is an explicit error naming `inp2p.c`/`devices/cpl/`.
- `len` is the instance spelling C probes for (`INP2Y` compares the token to
  `"len"`, not `"length"`); `length` on the instance is not a C setter and is
  an error.
- Registry: `y` stays `pending` until TX3/TX4 land, then becomes a bounded
  entry with an accurate scope string; `tests/registry*` expectations move
  with it.

*Tests.* Grammar (four terminals, missing model, `len` vs `length`, unknown
setter), the five required-parameter errors, the CPL model type, the deck
writer round trip, parser snapshots.

### TX2 · The three-pi fallback expansion

*Goal.* C's high-resistance path, which is a real and useful R/C model, and
which is a *parser-time* device expansion (`INP2Y`), so in this port it
belongs with the other setup-time expansions (`urc.rs`).

- Trigger: `lenval && rval && lval && rval/lval > 1.6e10`, where C re-lexes
  the *model card's own line* to obtain `rval`, `cval`, `lval`, `lenval`.
- Values: `r = 3/(r len)`, `c = c len/6`; three series resistors
  `node1 → txlnd1<name> → txlnd2<name> → node2` and four capacitors
  `c, 2c, 2c, c` from those four nodes **all to `gnode1`** (not `gnode2`).
- Names, which appear in rawfiles and must match: internal nodes
  `txlnd1<name>`, `txlnd2<name>`; instances `txlres1..3<name>`,
  `txlcap1..4<name>`. The `Y` instance itself is **not** created on this path.
- DC/AC/transient behaviour then comes from the R/C devices the port already
  has. Verified with C: a fallback deck reports `txlnd1y1`, `txlnd2y1` and the
  three `txlres*`/four `txlcap*` instances in `.op`.

*Tests/goldens.* A fallback deck in `conformance/netlists/` with `.op`/`.dc`
and a transient; analytic checks of the ladder; the generated-name
expectations.

### TX3 · The Padé path: DC only, plus observations

*Goal.* Everything about TXL that is well defined and cheap.

- Two branch-current rows `<name>#branch1`/`<name>#branch2`
  (`CKTmkCur` in `TXLsetup`), i.e. four unknowns per instance.
- The `MODEDC` stamp as C writes it: `0.1 * gmin` on the four node-node
  entries, `Ibr1-Ibr1 = 1`, `Ibr1-Ibr2 = 1`, `Ibr2-Ibr1 = −R len`, `pos-Ibr1 =
  1`, `neg-Ibr2 = 1`, `Ibr2-pos = 1`, `Ibr2-neg = −1` — the C form, not a
  re-derived equivalent.
- Parameters `pos_node`, `neg_node`, `length` (`TXLpTable`); `length` resolves
  instance-then-model exactly as `ReadTxL` does.
- `.ac`, `.sp`, `.tran`, `.pz`, `.noise`, `.disto`, `.sens` for a `y` device:
  explicit errors, each naming its C reference and, for AC/transient, the
  defect record from TX0.

*Tests/goldens.* `m10_txl_op`/`m10_txl_dc` from the C binary; the
`v(p) − v(n) = R len i1` relation; branch-current names and signs; every
refusal; registry status.

### TX4 · The Padé path: transient (decision-gated by TX0)

*Goal.* `txlload.c`'s recursive convolution, with C's coefficient setup.
In scope by decision 2; AC is refused by decision 1.

- TX4a `txlsetup.c`'s maths: `y_pade`, `exp_pade`, `get_h3`, `update_h1C_c`,
  `div_C`, `expC`, `multC`, the cubic root finders and the 3×4 Gaussian
  eliminations. These are self-contained numerics and can be unit-tested
  against the C values printed from the reference build.
- TX4b `txlload.c`: `add_new_vi_txl`, `get_pvs_vi_txl`, `right_consts_txl`,
  `update_cnv_txl`, `update_delayed_cnv_txl`, `copy_tx`, and the stamping
  (including the `tx->lsl` branch and the `tx->ext` coupling switch).
- TX4c `txlacct.c` (history recorded on accepted points) and the
  `CKTmaxStep = 0.9 * taul` bound, through the existing `src/devices/delay.rs`
  machinery if the shapes match.
- TX4d goldens: `m10_txl_tran` from `txl2_3_line`-shaped decks, plus the
  late-step deck from TX0; the decoupled early-step case documented, not
  implemented as a mode.

*Reuse.* `src/devices/delay.rs` (delay history, device breakpoints, step
bounds) and `src/devices/ltra/convolution.rs` are the closest existing
machinery; whether TXL's recursive form fits them or needs its own module is
TX4's first decision, to be recorded in `TXL.md`.

### TXL slice cost

TX0/TX1/TX2/TX3 are bounded and independent: TX1 is grammar plus schema,
TX2 is an expansion next to `urc.rs`, TX3 is a DC load and refusals. TX4 is
the only large slice and is the one the design decision can remove.

## Part 2 — MESA, HFET1, HFET2 (`z` levels 2–6, #83 remainder)

`mesa/`, `hfet1/` and `hfet2/` are three copies of one macspice lineage — the
same fourteen file names, the same `SPICEdev` shape, the same internal-node
scheme, the same temperature/trunc/ask/pz split. The existing `mesfet.rs`
covers `mes/` only, so these three get the treatment `mos.rs` already got: one
module, `src/devices/hfet/`, with a shared shell and one channel function per
level (decision 4).

### H0 · Shared shell and `z` dispatch

*Goal.* One place that owns the three families, and a `z` dispatcher that no
longer treats everything above level 1 as "not yet ported".

- `inpdomod.c` maps `level` to a device: 0/1 `mes` (already ported), 2/3/4
  `mesa`, 5 `hfet1`, 6 `hfet2`. `inp2z.c` accepts model types `MES`, `MESA`,
  `HFET1`, `HFET2`, and its leading unlabeled value becomes `area` **only**
  for the non-`MES` types (the port already refuses it for MES; the rule
  changes for the new levels).
- The `nhfet`/`phfet` model bases, which C routes through the same switch and
  the port currently refuses, must be accepted as those levels' polarity
  flags.
- Shared: instance defaults (`l = 1e-6`, `w = 20e-6`, `m = 1`), the
  conditional internal nodes (`#source`, `#drain`, `#gate`, then `#gs`/`#gd`
  for `ri`/`rf`, and MESA/HFET1's extra drain prime-prime for `rdi`), the
  temperature handling, `trunc`, `getic`, `ask`, `acLoad`/`pzLoad`, and the
  Newton limiting.
- **No noise, no distortion, no sensitivity** in any of the three: their
  `SPICEdev` entries leave `DEVnoise`, `DEVdisto` and `DEVsen*` `NULL`
  (`mesainit.c`, `hfetinit.c`, `hfet2init.c`). Note this differs from `mes/`,
  which has `mesnoise.c` and a ported noise model. Refusals must say so.

### H1 · MESA level 2 (`mesa1`)

- Model parameters (`MESAmPTable`, `mesa.c`; defaults in `mesasetup.c`,
  `level` defaulting to 2): `vto`/`vt0`, `lambda`, `lambdahf`, `beta`, `vs`,
  `rd`, `rs`, `rg`, `ri`, `rf`, `rdi`, `rsi`, `phib`, `phib1`/`tphib`,
  `astar`, `ggr`, `del`, `xchi`/`tggr`, `n`, `eta`, `m`, `mc`, `alpha`,
  `sigma0`, `vsigmat`, `vsigma`, `mu`, `theta`, `mu1`, `mu2`, `d`, `nd`,
  `du`, `ndu`, `th`, `ndelta`, `delta`, `tc`, `tvto`/`alphat`, `tlambda`,
  `teta0`, `teta1`, `tmu`, `xtm0`, `xtm1`, `xtm2`, `ks`, `vsg`, `tf`, `flo`,
  `delfo`, `ag`, `rtc1`, `rtc2`, `zeta`, `level`, `nmax`, `gamma`, `epsi`,
  `cas`, `cbs`, `pmf`, `nmf`, plus the outputs `gd`, `gs`, `vcrit`.
- Instance parameters (`MESApTable`): `off`, `l`, `w`, `m`, `icvds`,
  `icvgs`, `td`, `ts`, `dtemp`, and the node/observation quantities.
- Channel `mesa1` (`mesaload.c:499`); temperature `mesatemp.c`; AC
  `mesaacl.c`; pole-zero `mesapzl.c`; truncation `mesatrunc.c`; initial
  conditions `mesagetic.c`; observations `mesaask.c`, `mesamask.c`.
- C regression decks exist and give the goldens: `tests/mesa/mesa.cir`,
  `mesa11..15.cir`, `mesgout.cir`, `mesinv.cir`, `mesosc.cir` (all level 2).

### H2 · MESA levels 3 and 4 (`mesa2`, `mesa3`)

Same shell and the same model table; the level selects `mesa2`
(`mesaload.c:690`) or `mesa3` (`mesaload.c:858`), which differ in the mobility
and threshold expressions and read different parameter subsets. Ship as one
slice per level with a golden each. `tests/mesa/mesa21.cir` covers level 3;
**there is no level 4 deck in the C tree**, so level 4 needs a new deck of our
own plus a C golden.

### H3 · HFET1 (level 5)

- Model parameters (`HFETAmPTable`, `hfet.c`): ~60, including the
  `js1d`/`js2d`/`js1s`/`js2s` and `m1d`/`m2d`/`m1s`/`m2s` pairs, `cm3`, `a1`,
  `a2`, `mv1`, `kappa`, `delf`, `fgds`, `cds`, `phib`, `talpha`, `mt1`,
  `mt2`, `ck1`, `ck2`, `cm1`, `cm2`, `astar`, `eta1`/`d1`/`vt1`,
  `eta2`/`d2`/`vt2`, `ggr`, `del`, the integer `gatemod`, `klambda`, `kmu`,
  `kvto`, plus `nhfet`/`phfet`.
- Channel `hfet1load.c` (801 lines); setup `hfetsetup.c`; temperature
  `hfettemp.c`; pole-zero `hfetpzl.c`; asks `hfetask.c`, `hfetmask.c`;
  truncation `hfettrunc.c`; initial conditions `hfetgetic.c`.
- Goldens: `tests/hfet/id_vgs.cir` and `tests/hfet/inverter.cir` (both
  `nhfet level=5`), which have `.out` references.

### H4 · HFET2 (level 6)

- Model parameters (`HFET2mPTable`, `hfet2.c`): ~40, including `cf`, `d1`,
  `d2`, `del`, `delta`, `deltad`, `di`, `epsi`, `eta`, `eta1`, `eta2`,
  `gamma`, `ggr`, `js`, `klambda`, `kmu`, `knmax`, `kvto`, `lambda`, `m`,
  `mc`, `mu`, `n`, `nmax`, `p`, `rd`, `rdi`, `rs`, `rsi`, `sigma0`, `vs`,
  `vsigma`, `vsigmat`, `vt0`/`vto`, `vt1`, `vt2`.
- Channel `hfet2load.c`; setup `hfet2setup.c`; temperature `hfet2temp.c`;
  pole-zero `hfet2pzl.c`; asks `hfet2ask.c`, `hfet2mask.c`; truncation
  `hfet2trunc.c`. Internal nodes are only `#source`/`#drain` here.
- **No level 6 deck exists in the C tree**: the slice needs a new deck and a
  C golden.

### H5 · Documentation, registry and CLI

- `docs/port/MESFET.md` becomes the `z` guide with one section per level, or
  splits into `MESFET.md` + `MESA.md` + `HFET.md`; either way the level table,
  the parameter lists and the deliberate refusals live there.
- `MODEL_SCHEMAS.md`, the registry scope string for `z`, the `spice-rs
  devices` row, `README.md`'s device table and `TODO.md`'s device counts move
  with the code, as wave 3 did.
- The `.disto`/`.sens`/`.noise` refusals per level get the same treatment the
  MESFET port gave `mes/`, including the C-side reason (`NULL` entry points,
  not "unimplemented here").

## Ordering and dependencies

```
TX0 ──┬─ TX1 ── TX2
      │     └── TX3 ── TX4 (only if the TX0 decision keeps transient)
      └─ (AC/transient policy recorded)
H0 ──┬─ H1 ── H2 (level 3, then level 4)
     ├─ H3
     └─ H4
H5 trails each device slice, not the whole part
```

- TX1/TX2/TX3, H0 and the MESA/HFET channel slices all start from `main` and
  touch different files except `schema.rs`/`registry.rs`/`factory.rs`, so at
  most one writer should hold those at a time.
- H1 is the right first HFET/MESA slice: it has C regression decks, the
  simplest internal-node set of the three, and it establishes the shell.
- TX4 should not start before TX0's policy decision, and it is the only slice
  that can be dropped without losing the rest of #85.
- BSIM3 (#90) is independent of all of this and keeps its own plan.

## Definition of done (per slice)

The M10 checklist applies unchanged: `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked` (`+1.89.0` too when dependencies or
compiler-facing code change), `cargo xtask golden verify` with new goldens
reviewed and existing ones untouched, opt-in `NGSPICE_BIN` checks for the new
fixtures, `git diff --check`, and status documents updated only for what
actually works.

## Decisions (2026-10-15 review)

1. **TXL AC is refused.** `.ac` and `.sp` of a `y` device are explicit errors
   naming `txlload.c` and this document, with C's measured numbers as the
   reason. The port does not reproduce frequency-independent residue.
2. **TXL transient is in scope** (TX4 stays in the plan), gated on the
   settled-history regime being covered by goldens. The decoupled early-step
   regime is documented as C's behaviour, not implemented as a mode.
3. **All five TXL model parameters stay required** (`r`, `l`, `c`, `g`,
   `length`), following `TXLsetup` rather than LTRA's zero defaults.
4. **One shared `src/devices/hfet/` module** owns MESA/HFET1/HFET2 with
   per-level channel files, mirroring `mos.rs` owning `mos1/2/3/6/9.rs`.
5. **Work starts at H0+H1** (shared `z` shell plus MESA level 2), because it
   has C regression decks and establishes the shell H2/H3/H4 reuse.
6. Still open: whether the `z` registry scope string reports levels as they
   land (the MOS precedent) or stays one combined sentence until all six are
   done. Defaulting to the MOS precedent — report what works — unless told
   otherwise.
