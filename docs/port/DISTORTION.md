# `.disto`: small-signal distortion analysis (#104)

Implemented for issue #104 (milestone M8) in `src/analysis/disto.rs` (driver)
and `src/devices/distortion.rs` (device description API, Volterra products and
the three-variable Taylor arithmetic), with the device routines in
`src/devices/nonlinear.rs` (diode), `src/devices/bjt/disto.rs` and
`src/devices/mos1/disto.rs`. C references, read as behaviour only:
`src/spicelib/analysis/distoan.c` (`DISTOan`), `cktdisto.c` (`CKTdisto`),
`dkerproc.c` (`DkerProc`), `dloadfns.c` (the kernel polynomials),
`dsetparm.c` and `parser/inp2dot.c` (`dot_disto`), `src/maths/deriv/*.c`
(`Dderivs`), and the device routines `dio/diodset.c` + `diodisto.c`,
`bjt/bjtdset.c` + `bjtdisto.c`, `mos1/mos1dset.c` + `mos1dist.c`.

```text
.disto {dec|oct|lin} pts fstart fstop [f2overf1]
Vxxx n+ n- ... distof1 [mag [phase]] distof2 [mag [phase]]
Ixxx n+ n- ... distof1 [mag [phase]] distof2 [mag [phase]]
```

## Method

`.disto` is a Volterra-series analysis around the operating point:

1. The operating point and the small-signal operators `A`, `E` are those of
   `.ac` (`analysis::ac::SmallSignal`: `CKTop`, `.nodeset`, the `MODEINITSMSIG`
   load). Every device then describes its nonlinearities **at the bias** as
   Taylor polynomials (C `D_SETUP`, see [Device API](#device-api)).
2. At every `f1` of the sweep the system `A + j omega E` is solved for the
   first-order kernel `H1(f1)`, driven by the sources' `distof1` inputs as the
   phasors `0.5 mag e^(j pi phase/180)` (`cktdisto.c` `D_RHSF1`; a missing
   magnitude is 1, a missing phase 0, as `vsrcpar.c`/`isrcpar.c` set them).
3. Each higher-order kernel is the solution at its own frequency of a
   right-hand side built from the lower-order kernels: for every device term,
   the `dloadfns.c` polynomial of its control voltages is subtracted at the
   term's first node and added at its second, a charge's times `j omega` of the
   response being solved for. With `X`, `B`, `X2`, `M` the controls' values in
   `H1(f1)`, `H1(f2)` (conjugated for `-f2`), `H2(f1,f1)` and `H2(f1,-f2)`:

   | Product | Frequency | Second-order term `c_ij` | Third-order term `c_ijk` | Output scale |
   | --- | --- | --- | --- | --- |
   | `2f1` (`D_TWOF1`) | `2 f1` | `X_i X_j` | — | 2 |
   | `3f1` (`D_THRF1`) | `3 f1` | `X_i X2_j + X_j X2_i` | `X_i X_j X_k` | 2 |
   | `f1+f2` (`D_F1PF2`) | `f1 + f2` | `(X_i B_j + X_j B_i) / 2` | — | 4 |
   | `f1-f2` (`D_F1MF2`) | `f1 - f2` | same with `B = conj H1(f2)` | — | 4 |
   | `2f1-f2` (`D_2F1MF2`) | `2 f1 - f2` | `(2 (X_i M_j + X_j M_i) + B_i X2_j + B_j X2_i) / 3` | `(X_i X_j B_k + X_i X_k B_j + X_j X_k B_i) / 3` | 6 |

   The output scale is `DkerProc`'s conversion of the half-amplitude kernels
   to sinusoid amplitudes.
4. With `f2overf1` the second input `H1(f2)` is solved driven by the `distof2`
   inputs at `f2 = f2overf1 * fstart`, **fixed for the whole sweep** exactly as
   C does (`distoan.c`: "keeping f2 const to be compatible with spectre").
   `f1 - f2` and `2 f1 - f2` may be negative; the port solves a negative
   frequency as the conjugate of the positive one (exact for the real `A`, `E`).

The sweep is C's own, not `.ac`'s: `dec`/`oct` multiply by `exp(ln(10 or
2)/pts)` from `fstart` while `f <= fstop + delta * fstop * reltol`; `lin` adds
`(fstop - fstart)/(pts + 1)` while `f <= fstop + delta * reltol`, so a `lin`
card measures `pts + 2` frequencies (`lin 0` measures `fstart` and `fstop`,
or `fstart` once when they are equal). `reltol` is `.option reltol` or C's
default `1e-3`.

## Results

| Card | Plots, in order | Plot titles |
| --- | --- | --- |
| no `f2overf1` | 2 | `DISTORTION - 2nd harmonic`, `DISTORTION - 3rd harmonic` |
| `f2overf1` | 3 | `DISTORTION - IM: f1+f2`, `DISTORTION - IM: f1-f2`, `DISTORTION - IM: 2f1-f2` |

Every plot is complex, along the `frequency` scale (the swept `f1`), with every
node voltage and branch current named and ordered as an `.ac` plot (C writes
them with `CKTacDump`). In a batch the plots are named `disto<n>` and take
`disto`'s place in C's `analInfo[]` order (after `.tf`, before `.noise`); two
cards run in reverse deck order, so a deck with a harmonic card followed by an
IM card writes `disto1..3` (IM) and then `disto4 disto5`
(`batch::ScheduledAnalysis::extra_plot_names`, checked against ngspice-47's
`setplot` listing). `.save` and `.print disto` select vectors of every
distortion plot as they do for `.ac`. C marks only the first plot's scale
logarithmic for a
`dec`/`oct` sweep (`grid=3`); the port's rawfile writer writes no grid
attribute for any plot, as for `.ac`.

## Device API

```rust
pub enum DeviceDistortion {
    Linear,
    Input { f1: Option<DistortionInput>, f2: Option<DistortionInput> },
    Terms(Vec<DistortionTerm>),
}
pub struct DistortionTerm {
    pub response: Response,          // Current or Charge
    pub nodes: [NodeId; 2],          // the branch the response flows through
    pub controls: Vec<Control>,      // x, y, z: sums of node differences
    pub taylor: Taylor,              // xx..xz, xxx..xyz coefficients
    pub first_order_im3_kernel: bool // mos1dist.c quirk, see below
}
```

`Device::distortion(&DistortionContext)` sees only the operating point; its
default is `NotYetPorted`, so a device whose distortion is not ported is never
treated as linear. `Series3` is the truncated three-variable Taylor arithmetic
(`plus`, `times`, `mul`, `div`, `inv`, `sqrt`, `exp`, `tan`) standing in for
C's `Dderivs`; it keeps coefficients, not derivatives, and is checked against
finite differences.

| Device | C | `.disto` behaviour |
| --- | --- | --- |
| R, C, L, K, E/F/G/H (linear), S, W, model passives | no `DEVdisto` | `Linear`: only their AC matrix (a switch's on/off conductance at C's small-signal state) |
| V, I | no `DEVdisto`; `cktdisto.c` reads `distof1`/`distof2` | `Input` (`Linear` without inputs) |
| D | `diodset.c`, `diodisto.c` | junction current and charge in `v(a') - v(k)` |
| Q (Gummel-Poon) | `bjtdset.c`, `bjtdisto.c` | `ic(vbe, vbc, vbe)`, `ib(vbe, vbc)`, `ibb(vbe, vbc, vbb)`, `qbe(vbe, vbc)`, `qbx`, `qbc`, `qsc` |
| M (MOS1) | `mos1dset.c`, `mos1dist.c` | `id(vgs, vbs, vds)`, bulk diodes, bulk depletion and Meyer gate charges |
| B sources, `POLY`/`TABLE` code models | no `DEVdisto` | **refused**: C keeps their linearization and silently drops their nonlinearity |

C's device distortion models are their own simplified models, not derivatives
of the DC loads, and the port reproduces them as written:

* **Diode**: the ideal exponential of the total (bottom plus sidewall)
  saturation current, SPICE3's cubic reverse law below `-3 N Vt`, a breakdown
  exponential in `Vt` (not `NBV Vt`) below `-BV`; no ISR/IKF/IKR/tunnelling or
  `gmin` terms; the transit-time diffusion charge and the depletion charges
  graded against the model's **unadjusted** `VJ`/`VJSW` below the temperature-
  adjusted `FC * VJ` (applied to the sidewall too). The charge term is skipped
  when its second-order coefficient is zero (`diodisto.c`).
* **BJT**: junction currents with C's old reverse law below `-5 NF Vt`,
  `ISE`/`ISC` leakage, Early effect and `qb = q1 (1 + sqrt(1 + 4 q2)) / 2`
  (**no `NKF`**), evaluated at `vbe = v(b') - v(e')` and `vbc = v(b) - v(c')`
  (the **external** base); `RB`/`RBM`/`IRB` with `bjtdset.c`'s power-series
  inversion (which linearizes `vbb = rbb(ib) ib` around `ib = 0`); the
  `TF`/`XTF`/`VTF`/`ITF` diffusion charge divided by `qb`; B-E, split B-C
  (`XCJC`) and substrate depletion charges, the substrate one graded with the
  model's unadjusted `VJS`/`MJS` and always tied to the internal collector;
  `M` folded into the parameters with `bjtdset.c`'s extra `AREA` factor on the
  B-E depletion capacitance; second-order coefficients signed by the polarity,
  third-order and depletion ones not.
* **MOS1**: the Shichman-Hodges current with the body effect and C's
  source-drain interchange of the coefficients in inverse mode; forward-biased
  bulk diodes; bulk depletion charges whose second-order coefficient carries
  the polarity below `FC * PB` but not on the linear continuation; Meyer gate
  capacitances as single-variable charges of `vgs`, `vgd`, `vgb` (C's own
  comment calls them incorrect).

### Reproduced C defects

These are deliberate, documented parity choices, each confirmed against the
C binary (removing any of them breaks the C comparison):

| C | Defect | Port |
| --- | --- | --- |
| `cktdisto.c` | a current source's input enters as `rhs[pos] = -0.5 mag`, `rhs[neg] = +0.5 mag`, the opposite sign of its AC and DC stamps | `input_rhs` negates current inputs |
| `bjtdisto.c` | without a base resistance the `B-C'` (`qbx`) kernel is formed as `vbe + vbc` (the stale excess-phase `vbe` plus `vbc`), not `vbc` | `Control::sum([[b',e'],[b',c']])` (matters only with `XCJC < 1`) |
| `bjtdisto.c` `D_F1MF2` | the base-resistance (`vb - vb'`) and substrate (`vs - vc'`) kernels at `f2` are not conjugated (`i1hm2z` lacks its minus sign) | `Control::with_unconjugated_in_f1_minus_f2` |
| `mos1dist.c` `D_2F1MF2` | the `H2(f1,f1)` kernel is read from `H1(f1)` (`r1H1ptr` for `r2H11ptr`) | `DistortionTerm::with_first_order_im3_kernel` on every MOS1 term |

## Verification

* `tests/distortion_analysis.rs` (production path, closed forms): a diode
  biased by a current source against the exponential's Taylor coefficients —
  HD2 `2 h2`, HD3 `2 h3` with `h2 = -c2 h1^2 / g`, `h3 = -(2 c2 h1 h2 + c3
  h1^3) / g`, the classical `c2 a^2 / (2g)` second harmonic, `distof1` without
  a value, and the three IM kernels (1e-9 relative); a linear RC with zero
  distortion, C's names and the `lin` sweep's `pts + 2` frequencies; every card
  and device refusal.
* `src/devices/distortion.rs` unit tests: `Series3` against finite
  differences; `src/analysis/disto.rs`: plot counts and C's sweeps;
  `src/analysis/batch.rs`: `disto<n>` plot names in C's batch order.
* C goldens (`conformance/netlists/disto_*.cir`, batch fixtures in
  `cargo xtask golden verify` under `compare::DISTORTION`, 1 ppm relative with
  a 1e-18 floor): `disto_diode` (forward with RS/TT/sidewall at its own
  temperature, reverse-biased varactor with a phased current input, zener in
  breakdown; IM and harmonics), `disto_bjt` (NPN with IRB/XTF/VTF/ITF/XCJC,
  NPN with RB only, a substrate terminal, AREA/M/TEMP, PNP without RB; IM and
  harmonics), `disto_mos1` (saturated and linear NMOS, PMOS; IM and
  harmonics) and `disto_multi` (`.ac`, `.op`, a `lin` sweep with every linear
  device and S/W switches). Worst agreement with C on these decks is about
  1e-8 relative (at `reltol = 1e-7`).
* Opt-in live C (`NGSPICE_BIN=... cargo test --test c_disto_reference --
  --ignored`): the four fixtures plus a deck with a reverse-biased diode, an
  NMOS in inverse mode, a `lin 0` sweep and `f1 - f2 < 0`, and a PNP/NPN deck
  with AREA/M/TEMP, against ngspice batch mode (`-b -r`): plot sequence,
  flags, point counts, units and values by name (1 ppm, 1e-18 floor).

## Unsupported and divergent

Explicit errors where C would carry on: `dec`/`oct` with fewer than one step
(C loops forever), `fstop < fstart` (C writes empty plots), a sweep type other
than `dec`/`oct`/`lin`, more than 100,000 frequencies, a `hertz`-dependent
circuit (C linearizes once), B sources and XSPICE code models (C silently drops
their nonlinearity), and `f2overf1` without any `distof2` input (C's
`E_NOF2SRC`). Not supported: excess phase (`PTF`, already refused by the BJT),
`.measure` on distortion plots (rejected by the parser), and
distortion of
device families the port does not simulate (JFET, MESFET, MOS2/3/9, BSIM1,
VDMOS have `DEVdisto` in C). A circuit without any `distof1` input produces
all-zero plots, exactly as C does. The complex solves keep the `.ac` rank and
backward-residual guards (`ComplexLu::solve`), which can refuse some
well-posed reactive systems (#128); the fixtures avoid such values.

`.options keepopinfo` retains the preceding `Distortion Operating Point` plot.
