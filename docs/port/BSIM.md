# BSIM3 and BSIM4 (`M`, M11, #90)

**Status: plan only. No BSIM backend exists in the port.** Every BSIM level
currently fails with `NotYetPorted` naming the C directory `inpdomod.c` selects
(`src/devices/models.rs::mos_level_reference`), and model binning (#109) can
select a BSIM bin but has no factory to hand it to (`MODEL_SCHEMAS.md`).

This guide is the implementation plan for **milestone M11**. #90 moved off M10 so
that M10 closes on the device library defined in [M10.md](M10.md); the issue
keeps its acceptance criteria and lands here.

## Scope

1. **BSIM3v3.3 first** - `src/spicelib/devices/bsim3/`, which is what
   `level=8`/`49` with `version=3.3` or with no version string selects.
2. **BSIM4.8+ second** - `src/spicelib/devices/bsim4/`, the default for
   `level=14`/`54`; the issue's "BSIM4.7+" is covered by it being newer than
   `bsim4v7/`.

Explicit non-goals: BSIM-SOI (`bsim3soi_{fd,dd,pd}`, `bsimsoi`), HiSIM2/HiSIM-HV,
BSIM1/BSIM2 (`bsim1/`, `bsim2/`), and the older `bsim3v0`/`bsim3v1`/`bsim3v32`
and `bsim4v5`/`bsim4v6`/`bsim4v7` variants. A `version=` string that selects one
of those stays an explicit `NotYetPorted` naming its C directory - never a
silently substituted model.

## Size, measured

| C files | Lines |
| --- | --- |
| `bsim3/b3ld.c` (the DC/transient load) | 3,191 |
| `bsim3/b3temp.c` | 894 |
| `bsim3/b3set.c` | 1,205 |
| `bsim3/b3check.c` | 396 |
| `bsim3/b3mpar.c` + `b3par.c` (param plumbing) | 1,919 |
| `bsim3/b3acld.c`, `b3pzld.c`, `b3noi.c` | 1,191 |
| `bsim4v7/` replacement set (for scale) | 15,959 |
| `bsim4/b4ld.c` | 5,623 |
| `bsim4/b4mpar.c` | 3,685 |
| `bsim4/b4temp.c` | 2,408 |
| `bsim4/b4set.c` | 2,779 |
| `bsim4/b4check.c` | 935 |

Parameter tables, counted from the `IFparm` arrays: BSIM3 `b3.c` has **429 model**
and **19 instance** entries, BSIM4 `b4.c` has **897 model** and **44 instance**
entries. That is the whole reason this is a milestone and not a slice: the tables
alone dwarf every device the port has, and each entry needs a setter, a
temperature update and a load term.

## Why not reuse the `mos` shell?

`devices::mos` (`src/devices/mos.rs`) exists because MOS1/2/3/6/9 share ngspice's
classic MOS interface: four terminals, a normalized device frame, junction
geometry and a `mosXnoi.c`. BSIM3/BSIM4 share none of it:

- their own state layout (`CKTstate0` slots per `b3ld.c`/`b4ld.c`, not the
  classic `qgs`/`qgd`/`qbs`/`qbd`/`cd`/`cgm` shape);
- a different node set (drain/source primes plus the `b3trunc.c`/`b4trunc.c` LTE
  slots, and BSIM4's `rbodyMod`/`rbodymod` internal bulk node);
- `b3check.c`/`b4check.c` diagnostics that alter or reject cards;
- `capMod`/`mobMod`/`rdsMod`/`igcMod` mode selection inside the load, not a
  fixed equation set.

So BSIM gets its own module tree (`src/devices/bsim3/`, `src/devices/bsim4/`)
behind the same `Device` trait, exactly as `devices::hfet` does for the `z`
family. The `mos` shell stays untouched.

## Slices

Each slice is independently landable and leaves the branch green; BSIM3 and
BSIM4 repeat the same shape.

### B3-0 · Tables and schema (no load)

- Generate the model/instance schema from `b3.c`'s `IFparm` arrays rather than
  hand-writing 429 entries: a small generator in `xtask` (a checked-in
  intermediate, not a build dependency) emitting a `schema` table plus the
  `(keyword, C field, unit)` map the setters need. C's table order is part of the
  contract (it is the `.sens` order and the `show all` order), so the generator
  must preserve it.
- Wire `levels()` to accept 8/49 for the BSIM3 factory and dispatch it; keep the
  unsupported `version=` strings explicit.
- Deliverable: `.model` cards from real PDKs parse and validate with C's
  defaults, checks and `b3check.c` diagnostics; `.op` still fails explicitly.

### B3-1 · Temperature and instance setup

`b3temp.c` (894 lines) plus `b3set.c`'s geometry and *given*-flag handling:
`L`/`W`/`NF`/`SA`/`SB`/`SD`, `binUnit`, `m`, `dtemp`, and the temperature
scaling of `VTH0`, `K1`, `K2`, `UA`, `UB`, `UC`, mobility and series resistance.
Binning (#109) resolves `l`/`w` before this runs; the port's resolver already
implements C's inclusive 1 nm edges and last-declared-match rule.

### B3-2 · DC load (BSIM3 core)

`b3ld.c` without `capMod`: `b3setup.c`'s mode checks, the `mobMod` variants,
`vth`, `Abulk`, `Rd/Rs` (`rdsMod`), `Vgsteff`/`Vdseff`/`DITS`/`DIBL`/`CLM`,
subthreshold, and the Newton stamps. Validate against C by name with the usual
relative **and** near-zero absolute tolerances, over PDK-shaped cards.

### B3-3 · Charges and transient

`capMod=0/1/2/3` charge partitioning, `XPART`, the `b3trunc.c` LTE slots, and
the `.tran` companion path. This is where the HFET1 lesson applies: every
charge **derivative** slot must survive into the accepted history (see
[MESFET.md](MESFET.md#hfet1-level-5-z-m10-h3-83)), because trapezoidal order 2
reads it back and a zeroed slot is invisible in DC but wrong in every edge.

### B3-4 · Small signal, noise, pole-zero

`b3acld.c`, `b3pzld.c`, `b3noi.c`. `.disto` and `.sens` stay explicit refusals
unless a slice takes them (`.sens` for BSIM is the same `cktsgen.c` walk as
#164 and would be a fifth family of tables).

### B3-5 · Goldens and binning coverage

PDK-style fixtures: at least one deck per `capMod`/`mobMod`/`rdsMod` mode the
port claims, a binned `.<n>` family end to end (binning resolution -> factory ->
`.op`/`.dc`/`.ac`/`.tran`), and a `b3check.c` diagnostic deck. Capture with
`cargo xtask golden capture`, review every value, keep existing goldens
untouched.

### B4-x · BSIM4

Repeat B3-0..B3-5 against `bsim4/` (897 model entries, `rdsMod`/`rbodyMod`/
`igcMod`/`cgeomod` modes, `b4geo.c`, `b4soachk.c`). BSIM4's extra bulk-resistance
nodes and gate-current paths are the parts that do not lift from BSIM3.

## Guardrails

- C stays a read-only reference (`B3TERMS_OF_USE`/`B4TERMS_OF_USE` are in the C
  tree; do not copy code or headers). Behaviour is reimplemented in Rust and
  each function names the C file it follows, as everywhere else in the port.
- Unsupported modes, cards and `version=` strings fail with `NotYetPorted`
  naming the C file - never a substituted model, never a silently wider range.
- Generated tables are reviewed artefacts, not build output: they are committed
  with the C revision they were generated from, the way goldens record the
  binary that produced them.
- Ordinary tests need no C; PDK-deck comparisons remain opt-in via `NGSPICE_BIN`.
- Do not loosen tolerances or recapture existing goldens to make BSIM pass.

## First step

B3-0, because it is where the size is: if 429 schema entries plus the `(keyword,
field, unit)` map cannot be generated and reviewed, the rest of the plan needs
re-scoping before any load is written.
