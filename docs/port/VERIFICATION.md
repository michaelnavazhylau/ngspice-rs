# M8 follow-up regressions (#128–131)

Complex LU now retries failed original-system residuals with at most two
iterative-refinement corrections, for ordinary and transposed solves alike.
The `128 n eps` residual bound and complete-basis rank/conditioning cutoff
are unchanged. `tests/complex_solver.rs` retains explicit singular and
unresolved near-singular homogeneous rejection. Production regressions run
all 601 points of the 50-ohm LC ladder in AC and SP, and AC/adjoint noise
with the BJT emitter bypass raised to 10 uF at 10–100 MHz. Opt-in
`c_sparam_reference` and `c_noise_reference` compare them with live C;
the LC regression uses the existing AC bound (`1e-10 |C| + 1e-12`).
No golden was recaptured and no comparison tolerance was loosened.

The CLI and xtask batch paths schedule evaluated arguments, including equal
braced noise frequency bounds. `tests/simulate.rs` pins the resulting three
plot names for a mixed single-/multiple-frequency noise deck;
`c_noise_reference::equal_braced_noise_bounds_match_c` compares the rawfiles.
The no-analysis diagnostic is generated from `driver::DRIVERS` and its process
test requires every registered kind. Pole-zero preparation reuses
`ac::SmallSignal`, including deck DC options and nodeset/switch-state handling;
all existing pole-zero tests and live-C comparisons remain unchanged.

## M10 MOS shared shell and level 3 (#89)

The MOS1 frame moved into the shared `devices::mos` shell first, as a
separate commit with no behaviour change: every existing test and golden
passes unchanged, and every MOS1 deck's binary Rust rawfile (16 decks,
including `disto_mos1`, `noise_mos1`, `pz_mos1`, `m8_dc_param_mos1` and the
convergence/IC decks) is byte-identical before and after (dates aside).

Three new MOS3 C goldens, each captured with `cargo xtask golden capture
--netlist <name>` (no existing golden recaptured, no tolerance changed):

| Fixture | Gate | Result |
| --- | --- | --- |
| `m10_mos3_dc` | `compare::NONLINEAR` | 81 points |
| `m10_mos3_ac` | `compare::NONLINEAR` | 36 points |
| `m10_mos3_tran` | `compare::TRAN` | 393 instants + 16 breakpoint limits, worst 0.047 of bound |

Deck design, measured against the same C binary before capture:

- **Tight RELTOL for the AC bias.** With the default options C's Newton
  stopping test leaves the AC deck's drain bias 5.1e-6 relative from the root
  (0.5595758 V against 0.5595729 V), which the stage gain amplifies past the
  1 ppm bound (8e-5 absolute at 1 kHz). With `reltol=1e-7 vntol=1e-12
  abstol=1e-18` C and Rust agree to 6e-15 at the operating point, and the AC
  sweep then verifies: the port's `gm`/`gds`/`gmbs` are C's, including C's
  approximate channel-length-modulation derivatives.
- **Gear-2 and RELTOL 1e-5 for the inverter.** Under trapezoidal integration
  the gate currents ring at ±2.7 nA on the flat input with amplitudes that
  depend on each driver's step history (0.35% apart, 2.5x the bound). With
  Gear-2 at the default RELTOL the deck verifies at 0.966 of the bound; with
  `reltol=1e-5` (as `m7_ic_mos1_uic_tran`) at 0.047, and 0.037 at 1e-6, so the
  remaining difference is step control, not the model.

`tests/m10_mos3.rs` adds nine C-free tests; `devices::mos3` unit tests check
regions and finite-difference derivatives; `golden_rawfiles` documents the
three goldens (the m5 forward/reverse bulk current and the m6 reverse drain
current at vgs = 0 restate the cubic reverse law to the last digit). Opt-in
live C: `c_noise_reference::mos3_noise_matches_c` (both flicker forms) and
`c_pole_zero::a_mos3_stage_matches_c`. Six new parser snapshots were blessed;
existing snapshots are unchanged. See [M4_NONLINEAR.md](M4_NONLINEAR.md#mos3).

## Remaining M8 features (#132)

- `c_dc_param_sweep_reference`: remaining listed real instance setters, plus
  MOS1 AD/AS/PD/PS/NRS/DTEMP, BJT AREAC/TEMP and diode PJ; source sweeps through
  no-DC PORT accumulation. Linear comparisons use `1e-12 |C| + 1e-15`,
  nonlinear comparisons `1e-6 |C| + 1e-12` with tightened deck RELTOL.
- `c_sparam_reference`: passive and nonlinear noise covariances and two-port
  NF/SOpt/NFmin/Rn, hierarchical `#res` names, PORT DC/transient source order
  and bias plots. Covariance comparisons use a `1e-30` absolute floor rather
  than the voltage/current floor. Nonlinear circuit internal nodes omitted
  by C batch output are excluded from the layout comparison; every C vector
  is compared. Waveforms are checked against analytic values at both C's and
  Rust's physical sample times, including diffsol samples and a PWL corner.
- `c_noise_reference`: squared titles/names/units/values for voltage/current
  inputs and SPICE3 MOS1 flicker (C initialized with `ngbehavior=s3` before
  loading). `c_measure_reference` compares SP measurements with C using
  C's `vm(S_2_1)`/`vr(S_2_1)` syntax and its printed-value precision.
- `m8_additional_outputs`: CLI noise print routing, SP measurement/selection,
  retained bias, front-end setting round trips and explicit command errors.
  `sparam` checks both PORT transient backends analytically; `dae` checks exact
  cosine forcing, analytic derivatives, sample constraints and invalid terms.
- `keepopinfo` covers all C users: AC/SP, noise, distortion and pole-zero
  (C's PZ title is also `Distortion Operating Point`). Batch naming includes
  each preceding `op` plot; saves apply to bias plots. Live C checks include
  AC/PZ (`c_sparam_reference`) and distortion (`c_disto_reference`).

The tracked M8 follow-ups are implemented with the documented backend and model
bounds. No fixtures were recaptured or comparison thresholds relaxed. This does
not claim full SPICE parity. Front-end controls remain bounded pre-run settings;
diffsol retains its index-one structure restrictions and requires a piecewise
linear baseline beneath PORT cosines.

# Verification

## M10 URC lines (#85 part 1, `work/m10-urc`)

Three new C goldens (`m10_urc_tran`, `m10_urc_ac`, `m10_urc_diode_tran`),
each captured individually with `cargo xtask golden capture --netlist`; no
existing golden was recaptured and no tolerance was changed. `cargo xtask
golden verify` reports **123 verified / 0 unsupported / 0 failures** (AC at
`compare::AC`, both transients at `compare::TRAN`; the diode deck runs at
`reltol=1e-6`, worst error 0.12 of the bound, because at the default RELTOL
C's and the port's integration errors in near-zero source currents exceed
it); `golden check` reproduces all 123 fixtures. `cargo test --workspace
--locked` reports **1276 passed, 0 failed, 126 ignored**; all **126 ignored
live-C** checks pass with absolute `NGSPICE_BIN`, including the new
`c_urc_reference` (generated element values and `@u1[l]`/`@u1[n]` equal to
C's asks to 1e-15). Six new parser snapshots were blessed; existing snapshots
are unchanged. See [URC.md](URC.md).

## M8 sensitivity analysis (#102)

`.sens` DC and AC sensitivities ([SENSITIVITY.md](SENSITIVITY.md)).

- Five new fixtures, each captured on its own (`cargo xtask golden capture
  --netlist sens_divider|sens_hot|sens_diode|sens_ac|sens_multi`) after the
  deck was compared with the same C binary in a scratch copy; no existing
  golden was recaptured and no tolerance changed. `sens_multi` is a `BATCH`
  entry (`.ac`, `.op`, filtered `.sens`); the others are single-analysis
  entries under the new `compare::SENSITIVITY` (1e-8 relative, 1e-12
  absolute) and `compare::SENSITIVITY_NONLINEAR` (1e-6 relative, 1e-9
  absolute), justified in `xtask/src/compare.rs` by the `1/delta`
  amplification of rounding in a forward difference.
- `cargo xtask golden verify`: **119 verified / 0 unsupported / 0 failures**;
  `cargo xtask golden check --netlist <each new fixture>` reproduces all five.
- C agreement while developing (scratch decks, `ngspice -b -r` against
  `spice-rs simulate`): linear DC and AC decks agree to 1e-8 relative or
  better with every name in C's order; diode decks to about 4e-6 at C's
  default RELTOL (its operating point) and 1e-7 at RELTOL 1e-9, NaN where C
  reports NaN.
- `tests/sensitivity_analysis.rs` checks closed forms independent of C;
  the opt-in `tests/c_sens_reference.rs` (4 tests, 8 decks) compares C batch
  mode with names in order.
- Not verified: BJT, MOS1 and code-model sensitivities (refused), AC
  sensitivities of nonlinear devices (refused).

## M8 distortion analysis (#104)

`.disto` and the `distof1`/`distof2` source inputs ([DISTORTION.md](DISTORTION.md)).

- Four new fixtures, each captured on its own (`cargo xtask golden capture
  --netlist disto_diode|disto_bjt|disto_mos1|disto_multi`) after the deck was
  compared with the same C binary in a scratch copy; no existing golden was
  recaptured and no tolerance changed. All four are `BATCH` entries (an IM and
  a harmonic card each; `disto_multi` with `.ac` and `.op`) under the new
  `compare::DISTORTION` (1 ppm relative, 1e-18 absolute floor, justified in
  `xtask/src/compare.rs`).
- `cargo xtask golden verify`: **114 verified / 0 unsupported / 0 failures**;
  `cargo xtask golden check --netlist <each new fixture>` reproduces all four;
  `cargo xtask snapshots --bless` created only the eight token/AST snapshots
  of the new decks.
- C agreement while developing (scratch decks, `ngspice -b -r` against
  `spice-rs simulate`): ~1e-13 relative for diode and MOS1 decks, ~1e-12 for
  the BJT IM products and up to ~8e-8 for BJT harmonics of tiny branch currents at
  `reltol = 1e-7`; each reproduced C defect in DISTORTION.md was found as an
  exact sign or kernel mismatch and confirmed by the C source.
- `tests/distortion_analysis.rs` checks closed forms independent of C (diode
  exponential HD2/HD3/IM kernels to 1e-9); the opt-in
  `tests/c_disto_reference.rs` (2 tests, 6 decks) compares C batch mode by
  name.
- Not verified: excess phase (`PTF`, refused), the `keepopinfo` plot, device
  families without a port.

## M8 S-parameter analysis (#105)

`.sp` and RF port sources ([SPARAM.md](SPARAM.md)). The reference binary is an
`RFSPICE` build, so C data exists for every claim below.

- Three new fixtures were captured one at a time (`cargo xtask golden capture
  --netlist sp_attenuator|sp_rc|sp_multi`); no existing golden was recaptured.
  `sp_attenuator` and `sp_rc` are single-analysis `.sp` entries in `SUPPORTED`,
  `sp_multi` a `BATCH` entry (`.ac`, `.op`, `.tran`, `.sp`); all keep the
  existing bounds (`compare::AC`, `compare::DC`, `compare::TRAN`). The verify
  projection, which drops C-unsaved internal nodes, keeps the ports' `#res`
  nodes because C's `outitf.c` saves them. `sp_rc` has a shunt resistor so that
  no Z/Y component is a rounding-level real part.
- `cargo xtask golden verify`: **89 verified / 0 unsupported / 0 failures**;
  `cargo xtask golden check`: **89 fixture(s) reproduce the committed goldens**;
  `cargo xtask snapshots` created only the six token/AST snapshots of the new
  decks.
- `cargo test --workspace --locked`: **1124 passed / 0 failed / 75 ignored**;
  with `NGSPICE_BIN` and `-- --ignored` all **75** opt-in live-C tests pass,
  including the five of `tests/c_sparam_reference.rs` (C batch mode against
  `spice-rs simulate` by name, `1e-9 |C| + 1e-12`, on 10 decks).
- `tests/sparam.rs` (14 tests) checks closed forms independent of C; see
  [SPARAM.md](SPARAM.md#verification).
- Not verified: `donoise`, the transient `PORT` function and `.measure sp`
  (all `NotYetPorted`); the zero Y/Z block of a nonexistent matrix is a
  documented divergence from C's rounding-dependent values.

## M8 pole-zero analysis (#103)

Seven new C goldens, each captured once with `cargo xtask golden capture
--netlist <name>` after the deck was checked against the same C binary in a
scratch copy; no existing golden was recaptured and no tolerance changed.
`golden verify` compares pole-zero plots as unordered root sets under
`compare::POLE_ZERO` (`|Rust - C| <= 1e-6 |C| + 1e-9 max|C root|`, justified in
`xtask/src/compare.rs` and [POLE_ZERO_ADR.md](POLE_ZERO_ADR.md)); the plot name,
flags, variable names, units and root counts must match exactly.

| Fixture | Exercises | Worst error |
| --- | --- | --- |
| `pz_ladder_cur` | `cur pz`, grounded positive output (swapped drive), complex pair | 1.0e-9 of bound |
| `pz_bridge_diff` | differential output (column addition), zero at the origin | 6.4e-10 of bound |
| `pz_transformer` | K coupling, complex pair, zero at the origin | 7.2e-9 of bound |
| `pz_cv_loop` | capacitor across an ideal supply (index-two block) | 1.5e-10 of bound |
| `pz_diode` | junction linearized at the operating point, `rs` internal node | 1.3e-3 of bound |
| `pz_mos1` | MOS1 Meyer/overlap charge, pole at the origin, RHP zero | 5.3e-7 of bound |
| `multi_analysis_pz` | `.ac`, `.op`, two `.pz` (`pz1` zeros, `pz2` poles) in batch order | 7.0e-10 of bound |

`cargo xtask golden check` reproduces all seven with the C binary. After merging
`work/m8` (`.tf`, `.sp`, Gear 3-6, instance sweeps): `cargo test --workspace
--locked` **1185 passed / 0 failed / 90 ignored** (stable), `golden verify`
**105 verified / 0 unsupported / 0 failures**, **288** snapshots unchanged
(14 new parser snapshots of the decks were blessed), fmt and Clippy clean.
The five new opt-in `c_pole_zero` checks pass against ngspice-47+ (current input
at the output, a switch with a B source and a subcircuit, `zer`-only and
`pol`-only cards with a model resistor at 60 C, a Gummel-Poon stage, a
differential input through a subcircuit inductor, and the CCVS sign divergence:
the port matches C's result for the H gain negated).

Rust-only evidence: `tests/pole_zero.rs` (closed-form RC, series RLC, two-pole
ladder, highpass, input impedance, a notch whose three poles C's search does not
find, a CV loop without spurious poles, `ac 0` versus DC-only sources, empty
plots, bias-dependent diode poles, `PZinit`/card/singular/unsupported-device
errors, CLI layout and batch composition, `.save` refusal) and the unit tests of
`maths::pencil` and `analysis::pz`. Checked out of tree against LAPACK
(`scipy.linalg.eigvals` on dumped pencils): the port's roots agree to `1e-10`
relative or better; C stops at its own search tolerances (up to `1.5e-9`
relative on the goldens), and on a Gummel-Poon amplifier and a notch filter C
gives up with an iteration-limit warning and omits roots.

## Single-crate consolidation (`restructure/single-crate`, no functional change)

The six port crates (`spice-core`, `spice-netlist`, `spice-maths`, `spice-devices`,
`spice-analysis`, `spice-cli`) are consolidated into one package, `ngspice-rs`, so
that crates.io carries a single name and a single version. The former crate
boundaries survive as the module tree under `src/` (`primitives`, `netlist`,
`maths`, `devices`, `analysis`, `cli`); the CLI binary is `src/bin/spice-rs.rs`
and integration tests moved to `tests/*.rs` at the package root. This changes
layout, manifests and documentation only — no simulation path, tolerance, golden
or capability claim — so this section records regression evidence rather than new
coverage. Two inner modules were renamed to satisfy `clippy::module_inception`:
`analysis::analysis` is now `analysis::driver` and `cli::cli` is now
`cli::args` (`ngspice_rs::cli` still re-exports `Args`, `Command`, `exit_code`,
`run`, `usage`). See [ARCHITECTURE.md](ARCHITECTURE.md): dependency direction is
now a review-enforced convention instead of a compiler-enforced one.

- `cargo test --workspace --locked` reports **1051 passed / 0 failed / 67
  ignored** on stable (rustc 1.99.0) and on Rust **1.89.0**. That workspace total
  includes `xtask`, which stays a separate unpublished workspace member and
  contributes 48 tests; `ngspice-rs` alone reports **1003 passed / 0 failed / 67
  ignored**. All **67** ignored tests are the opt-in `NGSPICE_BIN` live-C checks,
  and none was rerun for a layout-only change.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` is clean on
  both toolchains; `cargo fmt --all -- --check` is clean and `cargo xtask ci`
  passes end to end.
- `cargo xtask golden verify` reports **59 verified / 0 unsupported / 0
  failures** with no exclusions, and `cargo xtask snapshots` reports **196
  snapshot(s): 0 created, 0 changed, 0 removed**. Neither needs a C binary, so
  together they re-establish that the moved tree still reproduces the committed
  data.
- Three doctests run (crate root, `devices::models`, `devices::passive`). The
  two device doctests had addressed the library through `crate::`, which is
  meaningless inside a doctest; they now use `ngspice_rs::`.
- Packaging: `cargo package` builds a 603-file `.crate` (2.0 MiB compressed) and
  the unpacked `.crate` runs its **own** suite — **1003 passed / 0 failed / 67
  ignored** — because conformance fixtures are now package-relative
  (`../conformance/...` from `tests/`). `cargo publish --dry-run` completes with
  a single upload. Release automation itself is not written yet; it is tracked in
  [TODO.md](../../TODO.md).
- After merging M7 (#121) into the consolidated tree (M7's new modules land in
  `src/devices/` and its tests in `tests/`; `transistors.rs`, which M7 emptied,
  is gone): **1110 passed / 0 failed / 70 ignored**, fmt and Clippy clean,
  `golden verify` **86 verified / 0 unsupported / 0 failures**, and the **70**
  opt-in `NGSPICE_BIN` live-C checks pass.

## M8 transfer function (#101)

Four new C goldens, each captured once with `cargo xtask golden capture
--netlist <name>` after the deck was checked against the same C binary in a
scratch copy; no existing golden was recaptured and no tolerance changed.
`golden verify` now reports **90 verified / 0 unsupported / 0 failures**, and
`golden check` reproduces each new golden.

| Fixture | Gate | Exercises |
| --- | --- | --- |
| `m8_tf_divider` | `compare::DC` | V input, `v(out)`, inductor short and capacitor open |
| `m8_tf_controlled` | `compare::DC` | I input, E and G stages, `i(vs)` output |
| `m8_tf_bjt` | `compare::NONLINEAR` | Gummel-Poon CE stage with RB/RBM/IRB, `reltol=1e-8` |
| `m8_tf_batch` | `compare::NONLINEAR` per plot | `.op` + two `.tf` (diode `v(d,dm)`, MOS1 `i(vdd)`) in batch order |

`m8_tf_bjt` first failed by 1.2e-4 relative with the port's exact Newton
Jacobian: `bjtload.c` stamps the bias-dependent base resistance as `gx` only, so
C's `.tf` (like its `.ac`) omits `d(gx)/dV`. `.tf` now reloads with
`TrialState::with_c_jacobian` and matches C (see
[TRANSFER_FUNCTION.md](TRANSFER_FUNCTION.md)); the Newton solves themselves are
unchanged. The opt-in `tests/c_tf_reference.rs` compares `ngspice -b -r` with
`spice-rs simulate` on 5 further decks (passive, current inputs, E/F/G/H,
nonlinear with `.op`/`.ac`, S/W switches inside their hysteresis band with and
without ON/OFF flags) with exact vector names; every value agreed within 1e-9
relative, well inside the 1e-6 nonlinear bound the test enforces.

- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets
  --locked -- -D warnings` are clean.
- `cargo test --workspace --locked`: **1128 passed / 0 failed / 75 ignored**
  (`ngspice-rs` alone 1080 / 0 / 75).
- With `NGSPICE_BIN` set, all **75** opt-in live-C tests pass, including the five
  new `c_tf_reference` tests.

## M8 DC parameter sweeps (#97)

What the reference binary sweeps was established first (see
[DC_SWEEPS.md](DC_SWEEPS.md#what-c-sweeps-97)): two nesting levels, sources,
resistors, `temp` and settable real *instance* parameters `@inst[param]`;
model parameters and `.param` names are rejected by C. Four new C goldens, each
captured once with `cargo xtask golden capture --netlist <name>` after the deck
had been checked against the same binary in a scratch copy; no existing golden
was recaptured and no tolerance changed. `golden verify` now reports 90
fixtures.

| Fixture | Gate | Covers |
| --- | --- | --- |
| `m8_dc_param_diode` | `compare::NONLINEAR` | `@d1[area]` (RS internal node) x circuit `temp`, 24 points |
| `m8_dc_param_mos1` | `compare::NONLINEAR` | `@m1[w]` x `@m1[l]`, RSH drain/source nodes, 15 points |
| `m8_dc_param_gain` | `compare::DC` | `@g1[gain]` scaled by the card's `m` x `@e1[gain]`, 15 points |
| `m8_dc_res_temp` | `compare::DC` | `.dc r1` x `temp` on a model-backed resistor (`res-sweep` scale), 6 points |

The nonlinear decks set `.options reltol=1e-8`: at C's default the
warm-started diode points stop ~3e-4 V short of the root. `golden verify` now
also projects C's `res-sweep` (type `res-sweep`) and `v(param-sweep)` (type
voltage) scales onto the Rust `sweep` column. The opt-in
`c_dc_param_sweep_reference` runs 15 further decks against the live binary
(linear at `1e-12 |C| + 1e-15`, nonlinear at the `NONLINEAR` bound); it also
detects C's BJT AREAB quirk (a variant that rescaled AREAB with AREA failed at
`v(cc)`, 0.122 V versus C's 0.106 V).

- `cargo test --workspace --locked`: **1119 passed / 0 failed / 73 ignored**;
  fmt and Clippy (`-D warnings`) clean.
- `cargo xtask golden verify`: **90 verified / 0 unsupported / 0 failures**;
  `golden check` reproduces all 90 fixtures; `cargo xtask snapshots`: 258
  snapshots, 8 created for the new decks.
- With an absolute `NGSPICE_BIN`, all **73** opt-in live-C checks pass,
  including the existing `c_dc_sweep_reference`.

## M7 nonlinear initial conditions (#99)

Six new C goldens, each captured once with `cargo xtask golden capture
--netlist <name>` after the deck was designed and checked against the same C
binary in a scratch copy; no existing golden was recaptured and no tolerance
changed. `golden verify` now reports 86 fixtures.

| Fixture | Gate | Worst error |
| --- | --- | --- |
| `m7_ic_diode_uic_tran` | `compare::TRAN` | 401 instants, 0.047 of bound |
| `m7_ic_bjt_flipflop_tran` | `compare::TRAN` | 117 instants + 8 breakpoint limits, 0.058 of bound |
| `m7_ic_bjt_off_tran` | `compare::TRAN` | 117 instants + 8 breakpoint limits, 0.002 of bound |
| `m7_ic_mos1_uic_tran` | `compare::TRAN` | 795 instants + 12 breakpoint limits, 0.245 of bound |
| `m7_ic_latch_nodeset_op` | `compare::NONLINEAR` | 8.7e-5 of bound |
| `m7_ic_latch_mos1_ic_op` | `compare::NONLINEAR` | 6.1e-8 of bound |

The operating-point worst errors were measured with an out-of-tree script
applying the same `|Rust - C| <= 1e-6 |C| + 1e-12` bound to every value.

- **Diode `uic`.** C skips the operating point; the first row is the first
  accepted step. Capacitor `c1` starts at its instance `ic=2`, `c2` and the
  diode junction (CJO, TT and RS, so the junction sits on an internal node)
  from the `.ic` node vector, which puts the junction at -0.5 V. The diode's
  own `ic=0.4` has no effect in C: `dioparam.c` never sets `DIOinitCondGiven`,
  so `diogetic.c` always replaces it with the node difference (the reference
  binary writes bit-identical rawfiles with `ic=0.4`, `ic=0.9` and no `ic`;
  `nonlinear_initial.rs` pins the same for the port). At C's default `reltol`
  its own `v(b)` at 0.1 us is 5.1e-3 V (about 4.6 times the bound) away from
  its `reltol=1e-5` answer, so the deck sets `reltol=1e-5`.
- **`.ic` flip-flop.** The symmetric cross-coupled BJT pair has two stable
  states and an unstable equilibrium; the `.ic` rows forced through the
  transient operating point select the q1-on state (`v(c1) = 0.2`,
  `v(c2) = 4` exactly at `t = 0`), then a reset pulse flips the pair. Default
  options.
- **`OFF` flip-flop.** A single-trigger pair with `q2 off`. C holds q2's
  junctions at 0 V through `MODEINITJCT`/`MODEINITFIX`, but `BJTconvTest` (run
  by `NIconvTest` under `NEWCONV`) keeps failing while the iterate's
  collector junction sits volts from the held 0 V, so the direct iteration
  exhausts `itl1` and dynamic gmin stepping lands in the q1-on state (without
  `OFF` the default schedule returns the unstable equilibrium). Reproducing
  the held-state convergence test is what makes the port take the same path;
  without it the direct iteration converged to the opposite state. A negative
  pulse on b1 flips the pair; `reltol=1e-6` keeps both step sequences tight
  through the flip (at the default the comparison is 0.53 of the bound).
- **MOS1 `uic`.** An inverter pair started from full and partial MOS1 `ic=`
  vectors and the `.ic` node vector: the bulk-junction and Meyer gate charges
  start at those voltages (C's first point moves from 1.78 V to 2.15 V with the
  vectors). Gear-2, because trapezoidal gate currents ring from step to step on
  the flat input (sign-alternating samples that no pointwise comparison can
  match), and `reltol=1e-5` with a 2 ps maximum step.
- **Latch operating points.** The symmetric CMOS latch's default point is the
  unstable equilibrium (`v(q) = v(qb) = 1.419 V`) in C and the port. A
  `.nodeset` forced in the `MODEINITJCT`/`MODEINITFIX` loads, or MOS1 `ic=`
  vectors that move the `MODEINITJCT` start (no `uic`), select the q-high
  state.

Opt-in live checks (`NGSPICE_BIN`) were not extended; the same scratch
comparisons also matched C for a forward-biased `OFF` diode (direct failure,
dynamic gmin, 0.6929 V), an `OFF` BJT driver, `.nodeset v(q)=1.6` (falls to
the q-low state) and an `OFF` CMOS latch (C's convergence test sends it to the
unstable equilibrium, which the port reproduces to 1e-8 of the bound).

Rust-only evidence: `tests/nonlinear_initial.rs`
(state selection by `.nodeset` and MOS1 `ic=`, exact forced `.ic` rows and
their release, explicit `.ic` contradictions, `uic` starts of D/Q/M and the
diode `ic=` quirk, `OFF` through dynamic gmin, `limiting=global` and BDF
rejections) and the `Limiter` unit tests in `devices::limiting`.

## M7 convergence parity (#106)

Seven new C goldens, each captured once with `cargo xtask golden capture
--netlist <name>` after the deck was designed and checked against the same C
binary in a scratch copy; no existing golden was recaptured and no tolerance
changed. `golden verify` now reports 80 fixtures. The decks are circuits whose
answer depends on the Newton path, so they test ngspice's algorithm (device
limiting, `MODEINITJCT`, `CKTop` strategies) rather than device equations alone.

| Fixture | Gate | Worst error |
| --- | --- | --- |
| `m7_conv_latch_op` | `compare::NONLINEAR` | 2.3e-8 of bound |
| `m7_conv_latch_gillespie_op` | `compare::NONLINEAR` | 0.0057 of bound |
| `m7_conv_latch_spice3_gmin_op` | `compare::NONLINEAR` | 3.3e-8 of bound |
| `m7_conv_latch_spice3_src_op` | `compare::NONLINEAR` | 0.24 of bound |
| `m7_conv_latch_tran` | `compare::TRAN` | 153 instants + 16 breakpoint limits, 0.016 of bound |
| `m7_conv_bjt_schmitt` (`.dc` down, `.dc` up, `.op`) | `compare::NONLINEAR` per plot | 0.028 of bound |
| `m7_conv_cmos_schmitt` (`.dc` down, `.dc` up, `.op`) | `compare::NONLINEAR` per plot | 0.0062 of bound |

The DC worst errors were measured with an out-of-tree script applying the same
`|Rust - C| <= 1e-6 |C| + 1e-12` bound to every value; `golden verify` prints
only pass/fail for point plots.

- **Latch.** The cross-coupled BJT pair has two stable states and a metastable
  one. C's default path (both junctions start at `tVcrit`, `DEVpnjlim`, dynamic
  gmin) and `gillespie_src`/`spice3_gmin` under `.options noopiter` all return
  the nearly balanced point; `spice3_src` (`srcsteps=4`) lands in a stable
  state. The Rust run reproduces each choice.
- **Schmitt triggers.** Each deck sweeps the input up and down through the
  hysteresis band (each `.dc` point warm-started from the previous one, as
  `dctrcurv.c` does) and solves an `.op` inside the band. For the BJT trigger
  C's `CKTop` lands on the middle (unstable) branch, which the port matches.
- **Tolerances.** The decks set `.options reltol=1e-8` (the CMOS one also
  `vntol=1e-12`; the transient `reltol=1e-7` with a 10 ns maximum step) so
  that C's own Newton stopping error is inside the bound, as for the #87/#88
  decks; even at `reltol=1e-6` the BJT trigger's sweeps differ by up to 1.3
  times the bound from the same root.
- **The new policies are load-bearing.** Running the decks with the port's
  previous step control (`limiting=global` request key) fails the BJT trigger
  by 7.2e5 times the bound and the CMOS trigger by 2.5e12 times (wrong
  hysteresis branch); with the previous ladders as well, the CMOS operating
  point does not converge at all. The pre-#106 build rejected `noopiter`. The
  default latch operating point is reproduced by the previous policies too; it
  pins the default path rather than discriminating between them.
- **Not gated**: C's `OPtran` fallback, which ngspice runs when every `CKTop`
  strategy fails (`.options noopiter gminsteps=0 srcsteps=5` on the BJT
  trigger at vin = 1.8 V: C prints "source stepping failed", then "Transient op
  finished successfully"), is not ported; the port's error names `optran.c`.

C-free coverage: `tests/convergence.rs` (limiter
functions, direct solve of an overdriven junction at an exact point, the
`MODEINITJCT` load, `noopiter`, the stage sequences of each strategy, schedule-
dependent latch states, the `optran.c` failure) and the `limiting.rs` unit
tests. Fourteen new parser snapshots were blessed; existing snapshots are
unchanged. See [DC_CONTINUATION.md](DC_CONTINUATION.md).

## M7 Gummel-Poon BJT (#87)

Five new C goldens, each captured individually with `cargo xtask golden
capture --netlist <name>` (no existing golden recaptured, no tolerance
changed): `m7_bjt_gummel` (VBC = 0 Gummel plot), `m7_bjt_output` (nested VCE/IB
output characteristics), `m7_bjt_temp` (`.dc temp` over NPN, lateral PNP with
substrate, TLEV=3/TLEVC=1), `m7_bjt_amp_ac` and `m7_bjt_amp_tran` (CE amplifier
at 50 C with every charge). They set `.option reltol=1e-8` because C's default
reltol with bypass stops these sweeps about 4e-4 from the converged root, far
outside the 1 ppm `compare::NONLINEAR` bound; with it the worst DC/AC errors are
below 0.1 of that bound and the transient's 0.039 of `compare::TRAN`.

`golden verify` projects two C naming conventions onto the Rust plot: a
temperature scale becomes `temp-sweep` (name and unit), and the Rust-only
`sweep(<outer>)` column of a nested sweep is dropped (C writes the points
without it; the outer value is still visible through the circuit's voltages).
The three new `c_bjt_reference` live-C comparisons are opt-in via `NGSPICE_BIN`. Ten new parser snapshots were blessed;
existing snapshots are unchanged. See [M4_NONLINEAR.md](M4_NONLINEAR.md#bjt).

## M7 diode physics (#86)

Five new C goldens, each captured individually with `cargo xtask golden
capture --netlist <name>` (no existing golden recaptured or tolerance changed),
are registered in `golden verify`: `m7_zener_dc` (a Zener
shunt regulator swept from forward conduction through reverse breakdown),
`m7_diode_physics_dc` (recombination, tunnelling, IKF/IKR/IKP knees, NS-sidewall
breakdown), `m7_diode_temp_dc` (`.dc temp -40 125 5`: EG/XTI/TNOM, TLEV 2,
DTEMP, TRS and TCV-shifted breakdown) and `m7_diode_temp_ac` (`.options
temp=100`: TLEVC 0/1 depletion laws, sidewall charge, recombination small
signal) under the 1 ppm `compare::NONLINEAR` bound, and `m7_zener_tran` (a
SIN-driven clipper through breakdown with junction, sidewall and diffusion
charge) under `compare::TRAN` (worst 0.470 of the bound). The DC decks set
`.options reltol=1e-6` (the regulator also `vntol=1e-9`): at C's default
tolerances its forward points stopped up to 0.3 % (in current) short of the
physical root, which the Rust point and an independent junction check satisfy
to 1e-10. The transient deck sets `reltol=1e-5` so both integrators resolve the
recovery at each zero crossing. These option lines were chosen before the
goldens were committed; the first captures of the default-tolerance drafts were
discarded, not committed. `golden verify` names C's `.dc temp` scale
`temp-sweep` (type `temp-sweep`, `dctrcurv.c`). Using the exact recombination
derivative in AC instead of C's stored conductance fails `m7_diode_temp_ac`
(6.9e3 times the bound). Details: [M4_NONLINEAR.md](M4_NONLINEAR.md#diode).

## M7 MOS1 completion (#88)

Four new C goldens, each captured once with `cargo xtask golden capture
--netlist <name>`; no existing golden was recaptured and no tolerance changed.
The existing MOS fixtures (`mos_inverter`,
`m4_mos1_ac`, `m4_mos1_tran`) verify unchanged (`m4_mos1_tran` still at 0.187
of its bound, although only the gate charges now enter LTE control, as in
`mos1trun.c`).

| Fixture | Gate | Result |
| --- | --- | --- |
| `m7_mos1_inverter_tran` | `compare::TRAN` | 393 instants + 16 breakpoint limits, worst 0.438 of bound |
| `m7_mos1_ring_tran` | `compare::TRAN` | 297 instants + 8 breakpoint limits, worst 0.523 of bound |
| `m7_mos1_meyer_ac` | `compare::NONLINEAR` | 36 points |
| `m7_mos1_process_dc` | `compare::NONLINEAR` | 31 points |

Deck design, measured before capture against the same C binary in a scratch
copy (never in the committed tree):

- **Bounded maximum step.** The Meyer charge `q1 + (v - v1) * average C` is a
  trapezoidal quadrature of `C(v) dv` along the accepted voltage path, so its
  error depends on the step sequence, which differs between the two adaptive
  drivers. With the default maximum step (`tstep`) the inverter differs from C
  by up to 1.09x the `TRAN` bound at a 22 nA supply-current sample; with
  `tmax = 2 ps` it is 0.44x. The decks set `tmax` (2 ps inverter, 0.5 ps ring)
  so the comparison measures model agreement, not each driver's discretization
  error.
- **Three-stage ring.** A free-running ring amplifies any per-step difference
  into phase drift, and `compare::TRAN` compares values point by point with a
  1e-3 relative bound and a 1 uV floor. Refining a five-stage unloaded ring
  from `tmax = 2 ps` to `0.5 ps` moves **C's own** waveform by 8.4 mV (Rust's by
  5.7 mV) at a 1.74 ns transition, more than the C-Rust difference at either
  step (3.9 mV and 1.3 mV, shrinking with the step). Five-stage variants still
  failed on sub-millivolt settling tails, where the bound is about 1 uV (worst
  37x for 50 fF loads at 1 ps, 224x unloaded at 1 ps, 302x for 20 fF at 2 ps):
  this is the two simulators' discretization error, not a model difference.
  Three stages with 50 fF loads at 0.5 ps verify over 6 ns (about ten periods)
  at 0.523x; the same deck at 1 ps fails at 1.34x, so the step bound is
  load-bearing. The kick is a single 0.3 ns current pulse into `n1`, because
  the ring's operating point is the metastable symmetric state.
- **Tight DC RELTOL.** With RD/RS and forward body bias, C's default Newton
  stopping test leaves its DC-sweep currents up to 2.2e-4 relative from the
  physical root: for an NMOS with `RS=90` and 0.3 V forward body bias, C gives
  -2.64450e-6 A at vgs = 0.2 V by default and -2.64508e-6 A with `.options
  reltol=1e-7 vntol=1e-12 abstol=1e-18`; Rust gives -2.645078e-6 A either way.
  The process deck therefore sets those options, which Rust also honours,
  rather than loosening `NONLINEAR`.

`tests/m7_mos1.rs` adds ten C-free tests (charge recurrences,
AC capacitances, temperature/process laws, series-resistance KCL, transient and
DC finite-difference Jacobians, `NotYetPorted` inputs and fixture round trips);
eight new parser snapshots were blessed and existing snapshots are unchanged.
See [M4_NONLINEAR.md](M4_NONLINEAR.md#mos1).

## M6 switches S/W (#81, `work/m6-switches`)

On top of the Wave 1 tree this slice adds six C goldens (`switch_op`,
`switch_dc`, `switch_dc_decimal`, `switch_ac`, `switch_tran`,
`switch_w_tran`, each captured individually; no existing golden recaptured):
`cargo xtask golden verify` reports **44 verified / 0 unsupported / 0
failures** and `golden check` reproduces the new fixtures. `cargo test
--workspace --locked` reports **963 passed, 0 failed, 63 ignored**; all **63
ignored live-C** checks pass with absolute `NGSPICE_BIN`, including the eight
`c_switches` tests (decimal-step and nested sweeps, AC with C's
`MODEINITSMSIG` state). Twelve new parser snapshots were blessed; existing
snapshots are unchanged. See [SWITCHES.md](SWITCHES.md).

## M6 Wave 1 integration (#78, #94, #95, #96, #107, #110)

The integrated `work/m6-common-decks` tree (source functions, `.func`/quoted
expressions, `.option` coverage, multi-analysis decks and linear E/F/G/H
controlled sources) reports **38 verified / 0 unsupported / 0 failures** in
`cargo xtask golden verify`; `golden check` reproduces all **38** C fixtures and
144 parser snapshots are unchanged by the merges. `cargo test --workspace
--locked` reports **931 passed, 0 failed, 55 ignored**, and all **55 ignored
live-C** checks pass with absolute `NGSPICE_BIN`. Per-slice counts in the
sections below record each slice's own state. A cross-slice CLI test
(`m6_features_combine_in_one_multi_analysis_deck`) runs `.func`/quoted gains on
E/G/H, a SIN source and DC-only options in an `.op` + `.tran` deck and checks
the operating point against values measured with ngspice-47.

## Bounded numerical follow-up gate (#46, #47, #29)

Parent validation of the integrated candidate tree reports **824 passed, 0 failed,
37 ignored** on stable (rustc 1.99.0) and Rust 1.89.0; combined maths **72/0/0**.
Workspace/all-target Clippy `--locked -- -D warnings` passes on both toolchains;
formatting and whitespace checks are clean. `cargo xtask golden verify` remains
**26 verified / 0 unsupported / 0 failures**; all **37 ignored live-C** checks
pass with absolute `NGSPICE_BIN`, and `golden check` reproduces all **26** C
fixtures. Existing goldens, 110 parser snapshots, dependencies and `Cargo.lock`
are unchanged.

- #46: **8** production-interface analytic scaling tests; independent physical
  values/residual bounds, snapshots, symbolic-pattern/rank/range errors and
  measured acceptance/overhead probes ([EQUILIBRATION.md](EQUILIBRATION.md)).
- #47: **6** guard tests, backend audit and example-only benchmark comparing
  actual production classification at widths 1/8/16. Parent reruns retain all
  **42 cases per width (24 accept / 18 reject)** and failed probes. Timing/RSS
  limitations and the aggregate-contraction proof caveat are explicit
  ([SPARSE_RANK_DIAGNOSTICS.md](SPARSE_RANK_DIAGNOSTICS.md)); formal certificate
  gate #68 remains unresolved. Numeric policy/thresholds are unchanged.
- #29: **11** analytic/error/residual/prototype tests plus unchanged index-one
  regressions. This is a numeric-only smooth constrained-RLC formulation gate,
  **not production enablement** ([HIGHER_INDEX_DAE_ADR.md](HIGHER_INDEX_DAE_ADR.md));
  separate waveform/topology/integration/event gates are #69–#72.

A fresh fourth read-only agent inspected exact candidates and actual logs:
no candidate-caused P0/P1/P2; all three **OK with notes**. Parent corrected
certification wording and carried inherited/prototype limits into capability
summaries before revalidation. Finite tests are not a universal uniqueness proof,
and normwise backward checks do not promise componentwise/forward accuracy.
The slice counts below are historical delivery evidence, not current totals.

## Multi-analysis batch decks (#96)

`spice-rs simulate` runs every analysis card of a deck in ngspice batch order and
writes one multi-plot rawfile ([CLI.md](CLI.md#multi-analysis-decks)). The C
golden `conformance/golden/multi_analysis_rc.raw` (deck order `.tran .ac .op
.dc`, captured with `cargo xtask golden capture --netlist multi_analysis_rc`; no
other golden was recaptured) holds four plots in C's order `AC Analysis`, `DC
transfer characteristic`, `Operating Point`, `Transient Analysis`.

* **Capture.** `write` alone writes only the current plot, so `xtask` instruments
  a deck with several analyses to `write <fixture>.raw ac1.all dc1.all op1.all
  tran1.all` — the C plot names in batch order, computed by
  `analysis::batch::schedule` — followed by `quit` (without it, batch mode
  re-runs the deck after `.endc` because of the `.op` card and fails). The
  capture refuses a rawfile whose plot count differs from the schedule.
  Single-analysis fixtures are instrumented exactly as before.
* **Verify.** `xtask/src/verify.rs` has a `BATCH` registry beside `SUPPORTED`:
  one `Stage` (analysis type and gate) per plot. The fixture's schedule, the
  golden's plot count and the stage count must agree; each plot must carry the
  same `Plotname:` as the C plot at its position, and is then compared under
  the same gates and tolerances a single-analysis fixture of its type uses
  (`compare::AC`, `compare::DC`, `compare::TRAN`). Single-plot checks are
  unchanged and still require exactly one C plot. Unit tests show that
  swapped, dropped and duplicated plots, a perturbed value and a deck whose
  schedule changed all fail.
* **C batch oracle.** `tests/c_batch_reference.rs` (opt-in)
  runs `ngspice -b -r` — genuine batch mode with a binary rawfile — and
  `spice-rs simulate` on the fixture and on a deck with two `.dc` cards, and
  requires identical plot count, order, names, flags and variables and values
  within 1e-9 relative + 1e-12 absolute. This ties the `.control` capture route
  to the batch-mode output it stands in for.

`cargo xtask golden verify` reported **27 verified fixture(s), 0 unsupported
fixture(s), 0 failure(s)** on the multi-analysis slice branch.

## M6 exit gate

The milestone's exit criterion is a representative deck that runs end to end
with every analysis it contains. `conformance/netlists/m6_gate.cir` drives a
1:2 K transformer (k = 0.98) with `SIN(0 0.1 1k)` into a non-inverting op-amp
stage: a subcircuit macromodel (G into 100 Meg || 159 pF, buffered by E;
DC gain 1e5, GBW 1 MHz) with `rf = {(gain-1)*rg}` from `.param`. Its output
feeds a `.func`-based B limiter `2 tanh(half(v(out)))` whose load current an H
source senses; `.option reltol=1e-4` applies to every analysis. The deck lists
`.tran .ac .op .dc`; C batch order gives four plots.

* **Golden.** Captured once with `cargo xtask golden capture --netlist
  m6_gate`; no other golden was touched. Registered in `BATCH`: AC, DC and OP
  under `compare::NONLINEAR`, the transient under `compare::TRAN` (301
  instants, worst error 0.077 of the bound). The transient breakpoint
  enumerator now accepts subcircuits without V/I waveform sources (a
  macromodel adds no breakpoints) and still refuses ones that contain them.
* **Circuit relations.** `tests/simulate.rs`
  (`the_m6_gate_deck_runs_every_analysis_end_to_end`) runs `spice-rs
  simulate` and checks, independently of C: the batch order and the C vector
  set; AC closed-loop gain 10 at 10 Hz and the limiter's unit small-signal
  slope; DC `v(out) = -9 vref` with the limiter and sense exact at every
  point; an all-zero operating point; and in the transient a ~2 V peak that
  the limiter compresses, with `v(lim) = 2 tanh(v(out)/2)` and
  `v(isense) = v(lim)/10` at every timepoint.

`cargo xtask golden verify` reports **59 verified fixture(s), 0 unsupported
fixture(s), 0 failure(s)** with the gate.

## Historical bounded M5 gate (#18, #6, #45, #42, #43, #44)

`cargo xtask golden verify` reports **26 verified fixture(s), 0 unsupported
fixture(s), 0 failure(s)**: `EXCLUDED` in `xtask/src/verify.rs` is empty, so
`subckt_divider` runs through the production `.op` path and matches its committed
C golden. `cargo test --workspace --locked` reports **799 passed, 0 failed, 37
ignored** on stable (rustc 1.99.0) and Rust 1.89.0; the 37 opt-in live-C
comparisons pass with `NGSPICE_BIN` set. Subcircuit instantiation:
[SUBCIRCUITS.md](SUBCIRCUITS.md); CLI `simulate` (one analysis, ASCII rawfile):
[CLI.md](CLI.md); binary rawfile read/write: [RAWFILES.md](RAWFILES.md);
`.save`/`.print` output selection: [OUTPUT_SELECTION.md](OUTPUT_SELECTION.md);
`.measure`/`.meas` measurements: [MEASURE.md](MEASURE.md); final-period `.four`:
[FOURIER.md](FOURIER.md). All six bounded M5 deliverables are complete
([ROADMAP.md](ROADMAP.md)); `.plot` and the documented extended forms remain
unported. Fourier C checks compare all nine harmonic magnitudes and at least
five significant phases per vector; a temporary +90° phase mutation failed all
three tests, then passed after restoring production code. Clippy on both
toolchains, formatting and whitespace checks pass; goldens, 110 parser snapshots
and `Cargo.lock` are unchanged by the Fourier slice.
The historical per-slice counts below record the state at each
delivery; they are not the current totals.

## M6 mutual inductance (#80)

Three new C goldens, each captured once with `cargo xtask golden capture
--netlist <name>` (no existing golden recaptured or tolerance changed), are
registered in `golden verify`: `transformer_ac` (linear AC bound),
`transformer_tran` and `transformer_ic_uic_tran` (`compare::TRAN`, worst error
0.000 of the bound); this slice reports 41 verified fixtures and `golden check`
reproduces the three. `transformer_tran` has no BDF variant because C's own
restart error after the 1 us pulse corner exceeds `compare::TRAN_RESTART`; the
BDF backend is instead checked against a reltol = 1e-7 companion reference.
A review follow-up added a fourth golden, `transformer_model_uic_tran`
(model-backed coupled inductors with instance `ic=` under `uic`, captured the
same way; 42 verified fixtures), after model-backed C/L gained their
truncation slot and `uic` storage element.
Opt-in live C:

```sh
NGSPICE_BIN=/abs/ngspice cargo test -p ngspice-rs --test c_mutual_inductance --locked -- --ignored
```

Details and measured analytic errors: [MUTUAL_INDUCTANCE.md](MUTUAL_INDUCTANCE.md).

## M6 source functions (#94, #95)

Five new C goldens, each captured once with `cargo xtask golden capture
--netlist <name>` (no existing golden recaptured or tolerance changed), are
registered in `golden verify` with `compare::TRAN`: `rc_sin_tran`,
`rc_exp_tran`, `rc_sffm_am_tran`, `rc_pwl_repeat_tran` and
`rc_pulse_count_tran` (31 verified fixtures in this slice, each worst error
0.000 of the bound). A discontinuous repeated PWL has no golden: at its
repetition boundaries C's single loaded value depends on ulp-level rounding of
its landing time, while the port always takes the left limit (see
[TRANSIENT.md](TRANSIENT.md#source-functions-94-95)). `tran::breakpoints` follows C's `VSRCaccept`: PULSE
corners up to `TD + NP*PER`, delayed and repeated PWL knots, none for
SIN/EXP/SFFM/AM. The SIN and EXP decks therefore add a constant PWL marker whose
knots make C land on the function's corners; without it the comparator would
interpolate C's plot across a slope corner that only the port lands on.

Opt-in live checks (`NGSPICE_BIN` absolute):

```sh
NGSPICE_BIN=/abs/ngspice cargo test -p ngspice-rs --test c_source_functions --locked -- --ignored
NGSPICE_BIN=/abs/ngspice cargo test -p ngspice-rs --test c_reference --locked -- --ignored
```

`c_source_functions` compares 22 V/I sources (every form and C default) with
C's node voltages at C's own timepoints (1e-9 relative; either limit at a jump
instant), the `.op` time-zero values, and the `.four` THD/harmonics of a
SIN-driven diode clipper (1 % + 0.05 points THD, 1 % + 1e-4 magnitudes).
`unmarked_decks_diverge_from_c_only_within_documented_bounds` runs RC decks
without marker sources in both engines and bounds the worst `v(out)` difference
(measured: discontinuous repeated PWL 0.024-0.036 V; SIN/EXP delay 2e-5/8e-5 V;
SFFM/AM delay jump 1.1e-3 V; EXP with `TD2 < TD1` 9e-3 V; fractional PULSE
count 1e-3 V).
`parsed_source_functions_match_live_c_coefficients` checks function codes and
coefficient vectors (`@dev[function]`, `@dev[coeffs]`) of
`conformance/parser/source_functions.cir`. Details:
[TRANSIENT.md](TRANSIENT.md#source-functions-94-95).

## M6 behavioural sources (#79)

Seven new C goldens, each captured once with `cargo xtask golden capture
--netlist <name>` (no existing golden recaptured, no tolerance changed), are
registered in `golden verify` (45 verified fixtures): `bsource_op`,
`bsource_dc`, `bsource_ac`, `evalue_op`, `gtable_dc` and `epoly_dc` under the
1 ppm `compare::NONLINEAR` bound and `bsource_tran` under `compare::TRAN`.
ngspice runs without `spinit`, so no XSPICE code model is loaded by default;
the TABLE and POLY decks name the libraries they need on a
`* xtask-codemodels: <name>...` comment and the capture (and `golden check`)
then writes a `.spiceinit` with just those `codemodel` commands into the
scratch directory (`NGSPICE_CODEMODEL_DIR`, the C build tree next to the binary
or `../lib/ngspice` are searched). Two findings shaped the decks: C's
operating points of chained XSPICE POLY sources are converged only to its own
`reltol` (about 5e-6 relative), so `epoly_dc` keeps POLY inputs on independent
sources; and with no breakpoints for B time functions both simulators' default
steps leave percent-level errors on a fast `sin(time)` drive, so
`bsource_tran` sets a 1 us maximum step.

Three more goldens, captured the same way, cover expressions whose slope at
the 0 V Newton start is about `1e32` (C's `PTdivide` fudge) or whose value is
`log(0) = -1e99`: `bsource_zero_op` and `bsource_zero_dc` (`NONLINEAR`) and
`bsource_zero_tran` (`compare::TRAN`; resistive loads and a 1 us maximum step,
because a capacitor current near its zero crossings differed by more than the
relative bound between the two simulators' step sequences). Registered, the
count is 48 verified fixtures.

Opt-in live check (`NGSPICE_BIN` absolute):

```sh
NGSPICE_BIN=/abs/ngspice cargo test -p ngspice-rs --test c_behavioural_reference --locked -- --ignored
```

It compares 47 B expressions (every `inpptree.c` function, operators, C's
derivative quirks and the 11-digit literal rounding) by value (OP) and
derivative (one-point AC) at four bias points with `1e-12` relative. Details:
[BEHAVIOURAL_SOURCES.md](BEHAVIOURAL_SOURCES.md).

## Branch-local M4 gate (#41)

The nonlinear support/gate is documented in [M4_NONLINEAR.md](M4_NONLINEAR.md).
`cargo xtask golden verify` verifies 26 fixtures (nine nonlinear), including
`subckt_divider` through the production `.op` path, with no exclusions. Six new C
AC/charge-transient goldens and twelve
parser snapshots were added without changing previous data. Physical/Jacobian/
charge/continuation/ownership checks live in `tests/m4_gate.rs`.
The historical linear sections below describe their original M2/M3 delivery;
they do not supersede M4's explicit supported-physics and tolerance table.

## The principle

The C tree is never linked. The port is compared against the reference
implementation **out of process**: `cargo xtask golden capture` drives the C
`ngspice` binary over a fixed set of decks and stores what it prints, and the
test suite reads those stored files. The workspace lint `unsafe_code = "forbid"`
exists to keep it that way — there is no FFI surface to widen by accident.

Two consequences worth stating plainly:

- `cargo test` needs neither the C tree nor a built `ngspice`. The goldens are
  committed data, so the suite runs anywhere.
- Drift is detected by re-running the reference implementation:
  `cargo xtask golden check`. Nothing in the port's own tests can detect that the
  C binary changed under it.

## Fixtures

`conformance/netlists/*.cir` are **pure decks**: no `.control` section, no
file I/O, and exactly one analysis card except for the deliberate
multi-analysis fixture `multi_analysis_rc` (#96). `conformance/netlists/README.md` explains why,
and the table there says what each fixture exercises.

## Capturing

`ngspice -b -r out.raw deck.cir` does not produce a usable rawfile here: with no
`set filetype` the output is binary, and `-r` is honoured before any control
script can change that. So `xtask` instruments each deck by inserting

```spice
.control
set filetype=ascii
run
write <fixture>.raw
.endc
```

immediately before the first `.end` card, runs `ngspice -b` on it with a scratch
directory (`target/xtask/golden/<fixture>/`) as the working directory, and reads
the ASCII rawfile that `write` produced. A deck with several analysis cards
writes every plot by name instead and ends the block with `quit` (see
"Multi-analysis batch decks (#96)"). A fixture that already contains a
`.control` section is rejected rather than instrumented.

The scratch directory is also why `write` takes a bare file name: the path never
needs quoting, and `.include`-style relative paths would resolve the same way
every time (they are out of scope for fixtures anyway).

## Storage and comparison

Goldens are stored **verbatim**: `conformance/golden/<fixture>.raw` is the byte
sequence `write` produced, minus nothing. Faithfulness matters more than tidy
diffs, and it means a golden can always be traced back to a command.

Comparison ignores exactly one line, the `Date:` header, because it comes from
the clock. Everything else is compared, including `Command:`, which carries the
`ngspice-<version>` string — so a change of reference binary is reported as
drift rather than silently absorbed.

## Commands

| Command | Effect |
| --- | --- |
| `cargo xtask golden capture` | capture every fixture, write `conformance/golden/*.raw`, report new/changed/unchanged |
| `cargo xtask golden check` | capture into the scratch directory and report drift; writes nothing, exits non-zero on drift |
| `cargo xtask golden list` | describe each committed golden: plot name, variable count, point count, finiteness |
| `cargo xtask golden verify [--netlist <NAME>]` | run the supported Rust analyses against committed C data; no C binary, no writes, non-zero on failure |
| `cargo xtask snapshots [--bless]` | check/regenerate token and AST snapshots; Rust only; non-zero on drift without `--bless` |
| `cargo xtask ci` | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` |

For **capture/check only**, locate the reference binary with `--ngspice <PATH>` or `NGSPICE_BIN`; otherwise
`build/src/ngspice` under the workspace root is tried (a convenience that only
exists inside a built ngspice checkout), then `ngspice` on `PATH`.
A relative `--ngspice` is resolved against the workspace root, not the caller's
directory, and stored as an absolute path so it still resolves inside the scratch
directory.

## What the tests check

`tests/golden_rawfiles.rs`:

| Test | Claim |
| --- | --- |
| `every_fixture_has_a_golden_and_no_golden_is_orphaned` | the two directories are in one-to-one correspondence |
| `every fixture is documented in EXPECTATIONS` | a new golden cannot be added without a human writing down what it contains |
| `fixtures_have_the_expected_shape_and_values` | plot name, flags, point count, variable names and hand-checked values |
| `goldens_are_well_formed_and_finite` | every plot has headers, variables with units, at least one point, and only finite values |
| `ngspice_lowercases_the_deck_title` | see the findings below |
| `the_writer_agrees_with_ngspice_apart_from_non_round_trippable_decimals` | the rawfile writer reproduces ngspice's layout line for line |
| `exactly_one_golden_token_is_not_round_trippable` | the one tolerated difference is bounded and named |
| `the_writer_reaches_a_fixed_point` | a second pass through the writer changes nothing |
| `the_transient_golden_matches_the_analytic_rc_curve` | the transient golden is physics, not just recorded numbers |
| `the_diode_golden_is_self_consistent` | sweep monotonicity, bounds, and Kirchhoff on the resistor |

The analytic and self-consistency tests matter as much as the value checks: a
golden captured from the wrong deck, or from a broken build, is internally
consistent but wrong. Checking it against a closed-form solution is what makes
the data trustworthy.

## Findings

Behaviours discovered while capturing, which the port has to reproduce. Each is
pinned by a test so that it cannot be lost.

1. **ngspice lowercases the deck title.** `Title: rc divider, operating point`
   for a deck whose first line is `RC divider, operating point`.
2. **The point index is not tab-separated.** `raw_write()` prints `" %d"`
   followed by `"\t%.*e\n"` for each value, and a blank line after every point.
   So the first value line of a point is `" 0\t<value>"` and the rest are
   `"\t<value>"`, with `""` between points.
3. **A complex plot has two spellings for a zero imaginary part.** A vector
   ngspice has flagged real is written `re,0.0`; one it has not is written
   `re,0.000000000000000e+00`. `.ac` flags nothing real, so even `frequency` and
   `v(in)` carry the long form — this is why `analysis::Variable` has an
   `is_real` flag rather than inferring it from the data.
4. **`%.15e` is not always a round-trip.** A rawfile value carries 16
   significant digits, and two adjacent doubles can share one 16-digit spelling.
   Exactly one token in the current corpus shows this: `time` on line 19 of
   `rc_transient.raw` is written `1.000000000000000e-11`, but the nearest double
   to that decimal prints as `9.999999999999999e-12`. Re-serialising a golden is
   therefore not always byte-exact, which is why the writer is checked by
   numeric comparison plus a fixed-point property. This does **not** weaken the
   port's parity goal: when the Rust engine computes the same double ngspice
   computed, the same `%.15e` formatting produces the same bytes.
5. **`TEMP` and `TNOM` default to 27 °C.** `Doing analysis at TEMP = 27.000000
   and TNOM = 27.000000` on every run, matching
   `AnalysisContext::default()`.
6. **A DC sweep writes a blank line between points**, which the rawfile parser
   has to skip; a blank line is therefore not a reliable plot separator inside
   the values section.

## M1a parser verification

`tests/linear_parser.rs` checks AST fields against the
committed divider, AC low-pass and RLC decks: terminal order, values, source
locations, request arguments, case folding and ground aliasing. It also checks
the unflattened `subckt_divider` AST (#12). `rc_transient` has a positioned
waveform AST test; the three nonlinear fixtures have M1b tests. All eight
fixtures parse without implying simulation or the M1 round-trip gate. `tests/parse.rs` checks
process exits: supported parse = 0, missing file = 2, unported syntax = 3.

A separate opt-in oracle uses `conformance/parser/linear_sources.cir` to compare
parsed scalar parameters with C's `print @instance[parameter]` after an `.op`
setup. It runs out of process in a unique temporary directory and uses no FFI:

```sh
NGSPICE_BIN=/path/to/ngspice cargo test -p ngspice-rs --test c_reference -- --ignored
```

This pins bare-AC defaults (magnitude 1, phase 0), implicit DC zero, source
leading-DC precedence, and R/C/L values and initial conditions. The last-set
parameter map from Rust is compared numerically with C instance queries. The
probe uses simple values; its tolerance is `1e-12` relative, with a `1e-12` scale
floor for zero. The shared oracle sets `numdgt=17` for scalar-query precision.
The tests are ignored by default so ordinary tests remain C-toolchain-independent.
It was run successfully against the local ngspice-47+ binary for M1a.

`conformance/parser/` is **not** part of the rawfile fixture corpus; do not add
`.raw` files there or confuse these instance-query checks with engine parity.
Token/AST snapshots (#21) are committed under `conformance/snapshots/` and
checked byte for byte by `tests/snapshots.rs`;
`cargo xtask snapshots` reports drift and `--bless` regenerates (Rust only, no C,
fixed point, never touches goldens). Schema, layout, path/Windows rules and the
schema-change procedure: `conformance/snapshots/README.md`. The eight-fixture
round-trip gate is `tests/m1_gate.rs` (#22); ordinary tests
never need C, and C parser oracles remain opt-in.

`tests/c_param_reference.rs` is a further ignored oracle for
the `.param`/expression grammar (#14): it folds parsed trees with a test-local
evaluator and compares the values with C's numparam, pinning precedence,
associativity and the leading-sign rules. Run it with
`NGSPICE_BIN=/abs/path/ngspice cargo test -p ngspice-rs --test c_param_reference -- --ignored`.
The fixture `conformance/parser/param_expressions.cir` is parsed by ordinary
tests and the CLI without claiming any value is resolved.

## Winnow backend regressions

The existing M1a contracts are retained; fixture/CLI expectations now also
include the M1b diode/BJT/MOS decks, and the live oracle shares its runner across probes.
`tests/winnow_parser.rs` adds checks that cuts preserve
terminal and missing-value diagnostics, optional slots cannot swallow overflow,
repetition cannot hide unported expressions, AC lookahead leaves following
keywords untouched, and trailing device tokens are never silently ignored.
Unicode-node diagnostics remain byte-column-based and separate parser calls
cannot leak backtracking state.

Cache the locked dependencies with `cargo fetch --locked` before an offline
check (`cargo test --workspace --locked --offline`). The zero-external-dependency
claim applies to the historical M0/M1a backend, not the `new-parsing` rewrite.

Worktree-only publication checks run via
`bash scripts/tests/publish-rust-only.sh`. They use disposable local repos and a
local bare remote to verify branch routing and dirty/mismatched-target refusal;
they never contact GitHub and are not part of `cargo xtask ci`.

## M1b model/diode slice verification

`tests/model_diode_parser.rs` covers the `diode_dc` AST,
model families/forms, raw first level, ordered duplicates, numeric model names,
forward/unresolved references, scalar geometry, ground aliasing, continuation
provenance, committed malformed/overflow errors and specific unsupported gaps.
At that slice's completion, CLI tests required six fixture parses; #8 now adds
`rc_transient`, bringing the count to seven at that stage. #12 adds
`subckt_divider`: all eight exited 0 for parsing at that stage; #18 later added
subcircuit elaboration, so the deck now simulates through the production `.op`
path.

The second ignored test in `c_reference.rs` instruments
`conformance/parser/model_diodes.cir`. Live C instance/model queries check
leading-area precedence, `perim`→`pj`, scalar geometry/IC/temperatures, implicit
area, duplicate model setter order and basic scalar model parameters. Both
probes pass against ngspice-47+. Neither probe proves selector rounding,
model resolution, backend availability, arbitrary keyword validity, derived
geometry/defaults or Rust simulation arithmetic. Those boundaries remain
explicit in the AST and architecture docs.

## M1b BJT/MOS slice verification

`tests/transistor_parser.rs` adds 18 regressions for Q/M
fixture ASTs, optional substrate vs earliest declared model, alpha model names
with digits and numeric node names, forward declarations, raw ordered scalars,
leading BJT area, MOS bulk/model collisions, omitted ports, overflow, unsupported
advanced flags/extra ports/binning, and byte-column provenance. #10 adds
separate valid/malformed IC vector and bare-flag regressions below. Declaration
indexing stops at `.end`, excludes unsupported scope bodies, preserves error
order, and has no state shared across parser calls.

The third ignored oracle instruments `conformance/parser/transistor_scalars.cir`.
It compares scalar setters and external terminal IDs after C setup with the
parsed port order (including C's grounded omitted Q substrate), and covers
keyword-like model names and ordinary model names containing digits. The fourth
ignored test pins ngspice-47+'s rejection of Q model names `123` and `123n`:
Rust emits Parse for the former and an explicit numeric-model gap for the latter.
All four live tests pass; these remain syntax/setup checks, not Rust simulation.
Selector defaults/rounding, family compatibility, scoped model resolution,
node/model `gnd`/`0` collisions, model defaults and advanced device arithmetic
are not proven by these probes.

## Passive syntax and model-schema verification

PR #51 merged #11/#17. This section records their initial validation; the
bounded passive elaboration section below records #19's additional checks.
`passive_models.rs` adds 11 syntax regressions, and `tests/models.rs`
adds 13 production-interface checks for top-level first-wins lookup, raw AST
immutability, missing/wrong families, model/node namespace collisions, first raw
versus rounded/applied levels, unsupported backends, nonfinite/range/cache errors,
ordered setters/provenance, diode/context defaults and atomic circuit failures.
A module doctest demonstrates the resolver/typed diode API.

The fifth parser C oracle queries `conformance/parser/passive_models.cir`,
checking R/C/L pre-/post-model scalar precedence, forward references, omitted
values and independent C geometry expectations. The new device oracle queries
`model_schemas.cir`, checking first model declarations, bounded diode defaults and
explicit setters, Celsius/Kelvin conversion, repeated diode integer setters and
first BJT/MOS selector choice. Device-query bounds are `1e-12` relative plus
`1e-24` absolute; solver/golden tolerances are unchanged. C ground/model collision
parity, nonlinear equations and advanced/scoped models remain unproven.

Local validation: **282 passed, 0 failed, 7 opt-in C tests ignored** on stable
(rustc 1.99.0) and Rust 1.89.0. All **7** opt-in tests also passed separately
against the read-only local ngspice-47+ binary. Formatting, all-target workspace
Clippy on both toolchains, warning-free rustdoc and `git diff --check` passed.
Default and selected golden verification stood at three supported/five excluded
fixtures at this slice; no goldens were recaptured or changed.

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo +1.89.0 test --workspace --locked
cargo +1.89.0 clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
NGSPICE_BIN=/absolute/path/to/ngspice cargo test --workspace --locked -- --ignored
cargo xtask golden verify
cargo xtask golden verify --netlist rc_lowpass_ac
```

These initial schemas did **not** enable passive or D/Q/M factories, and
parsing/validation is not nonlinear simulation parity. See
[MODEL_SCHEMAS.md](MODEL_SCHEMAS.md) for API contracts, bounded level policy and C
references; #19 now adds the explicitly bounded passive support below.

## Bounded passive elaboration verification

The current checkout implements #19 on merged PR #51 (#11/#17), pending merge.
`tests/devices_passive_models.rs` adds **12** checks for model/instance
precedence, raw AST immutability, R sheet/C area-perimeter formulas, missing or
invalid geometry, coefficient overrides, positive multiplicity/scale, sign and
finite/range/overflow errors, explicit unsupported setters, real stamping,
initial-condition retention and atomic failure across every circuit namespace.
`tests/analysis_passive_models.rs` adds **7** checks comparing model/literal
DC, source sweeps and complex AC; repeated default/nondefault TEMP/TNOM runs;
explicit TEMP/TNOM overrides; runtime errors; contextual RC BDF against its
analytic step; and logarithmic-AC endpoint arithmetic. A new module doctest
exercises contextual passive assembly.

Two opt-in tests in `tests/c_passive_models.rs` compare effective
values and production DC/complex AC against live C at both 27/27 and 77/22
Celsius. `conformance/parser/passive_elaboration.cir` exercises R/C geometry,
model/default/instance precedence, repeated setters, TC1/TC2, scale and
multiplicity. Query comparisons use **1e-12 relative + 1e-24 absolute**. Production
DC retains **1e-12 + 1e-15** and AC **1e-10 + 1e-12**; analytic BDF retains
**2e-5 V**. No comparison bounds or committed goldens were changed.

Live C exposed that capacitor DEFL is not applied to instances; it is now an
explicit gap and geometry requires instance L. It also exposed an existing AC
logarithmic integer-span roundoff bug that omitted the endpoint. A bounded
floating-arithmetic snap repairs the grid count; integer/noninteger regressions
and C AC checks pass without relaxing value tolerances.

Local validation: **302 passed, 0 failed, 9 opt-in C tests ignored** on stable
Rust 1.99.0 and MSRV 1.89.0. All **9** opt-in C tests also passed separately against
the read-only local ngspice-47+ binary. Both toolchains' all-target Clippy,
formatting, warning-free rustdoc, the RC example and `git diff --check` pass.
Default golden verification stood at three linear fixtures with five
explicit exclusions at this slice; selected AC verification reports one verified fixture.
Neither is full corpus parity.

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo +1.89.0 test --workspace --locked
cargo +1.89.0 clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
NGSPICE_BIN=/absolute/path/to/ngspice cargo test --workspace --locked -- --ignored
cargo xtask golden verify
cargo xtask golden verify --netlist rc_lowpass_ac
cargo run -p ngspice-rs --example rc_diffsol --locked
```

[PASSIVE_MODELS.md](PASSIVE_MODELS.md) lists the exhaustive setter/units/default/
formula table and deliberate gaps. Coil geometry, advanced passive forms, D/Q/M
arithmetic, IC/uic and SPICE trap/Gear parity remain unimplemented.

## Bounded waveforms, flags and IC syntax (#8 / #10)

`waveform_parser.rs` adds eight production-parser regressions for PULSE omissions,
PWL pairing, mixed/duplicate DC/AC/waveform setters, leading DC precedence,
byte-column continuation positions, finite/shape/delimiter errors and bounded
work. `rc_transient` unlocked the seventh fixture; #12 now enables all eight.
`flags_ic_parser.rs` adds nine regressions covering the exhaustive bare-flag
inventory, base tokens versus model tail flags, Q 1–2/M 1–3 IC arities,
component names/positions, scalar/vector duplicate order and leading area,
malformed/overflow/advanced forms and first-error/.end behavior.

#9 adds `tests/waveforms.rs` (deck binding, C defaults, limits,
merged lazy breakpoints), `tests/source_waveforms.rs` (parsed
PWL/PULSE decks through diffsol BDF, jump sampling, budgets) and opt-in
`parsed_pulse_rc_matches_c_on_requested_samples` / `parsed_pwl_rc_matches_c_on_requested_samples`
in `c_linear_reference.rs`.

`tests/parser_setters.rs` adds three tests proving invalid waveform and
nonlinear factory failures are atomic, that flags/IC vectors do not enable
initialization, and that scalar factories/schemas reject non-scalar AST kinds
even when forged with valid numeric text. Device API waveforms are unchanged.

Two new ignored oracles in `c_reference.rs` use `source_waveforms.cir` and
`flags_ic.cir`: C coefficient vectors, preserved PULSE field omissions, last
waveform/DC/AC setters, D/Q OFF, scalar/vector IC overrides and partial IC
fallthrough. Model family flags and MOS1 OFF are input-only in C; successful
setup checks those forms, not nonexistent scalar queries. No rawfile goldens or
solver tolerances changed. See [FRONTEND_VALUES.md](FRONTEND_VALUES.md) for exact
syntax, intentional stricter punctuation and observed C preprocessing limits.

```sh
NGSPICE_BIN=/absolute/path/to/ngspice cargo test -p ngspice-rs --test c_reference --locked -- --ignored
```

Local validation: **322 passed, 0 failed, 11 opt-in C tests ignored** on stable
Rust 1.99.0 and MSRV 1.89.0. All **11** opt-in C tests passed separately, including
all seven parser probes. Both toolchains' all-target Clippy, formatting,
warning-free rustdoc and `git diff --check` pass. `cargo xtask golden verify`
remains three verified/five explicitly unsupported fixtures at this slice; no goldens changed.

These are syntax/setup comparisons, not Rust waveform evaluation or nonlinear
simulation. Scoped/source syntax is implemented below. (Historical note: the
expressions, evaluation, options/globals, serialization, snapshots and the
eight-fixture round-trip gate were completed later, #14-#16, #20-#22.)

## Scoped/source syntax (#12 / #13)

`subcircuits.rs` and `sources.rs` test ordered nested scope storage, forward
model names, X/formal textual parameters, structural diagnostics, source-relative
includes/library selections, include-chain/section-boundary provenance,
canonical cycles/symlink aliases, repeated/diamond includes, depth/file/byte/card
limits and ordered failures/termination. `spice-rs parse` resolves the committed
multi-file `conformance/parser/sources/main.cir` probe and all eight rawfile
fixtures; `tests/structure_parser.rs` explicitly rejects simulation
and checks atomic X factory failures. The new textual parameter kind is also
rejected by scalar consumers. See [FRONTEND_STRUCTURE.md](FRONTEND_STRUCTURE.md).

An eighth opt-in parser oracle copies that multi-file source probe into a scratch
directory and queries C's selected model RSH after setup. It checks accepted
source/subcircuit syntax and library selection, **not Rust flattening or
simulation**. No committed rawfile goldens or solver tolerances changed.

Local validation for this slice: **342 passed, 0 failed, 12 opt-in tests ignored**
on stable and Rust 1.89.0; both all-target Clippy checks and formatting pass.
All **12 opt-in C tests** pass separately on stable, including all **8 parser
probes** and the new source probe. Warning-free rustdoc and `git diff --check` pass. `golden verify` still
reports three verified/five unsupported fixtures at this slice, naming
subcircuit **flattening/elaboration**, not parsing, as the blocker; #18 later
removed it (the fixture now verifies and `EXCLUDED` is empty).

## Petgraph topology verification

`devices::Circuit::topology()` is exercised by nine additional circuit
regressions: ground/unused nodes, separate node/device/row namespaces, port
order and repeated multiport edges, disconnected structural components,
zero-port devices, snapshot rebuilding after mutation, duplicate/dangling
mutations with unchanged numbering on failure, and deterministic deck order.
Existing device/container regressions remain in place.

`tests/mna_topology.rs` now calls the production
`SparseMatrix::coupling_graph()` instead of a hardcoded test-only graph builder.
Eight checks cover structural blocks, diagonal-only/empty rows, one ordinary
row-0 unknown, the empty matrix, duplicate cancellations and input immutability,
asymmetric/opposite-sign entries, fresh graphs after stamps/clear, and invalid
rectangular shapes. Connectivity uses petgraph algorithms, not custom traversal.

These tests prove structural projection correctness, **not** DC ground-path
validity, model/analysis-specific topology rules or numerical nonsingularity.
Graph extraction has no finite-value validation and does not implement a solver.
The parser/C-oracle and golden checks remain unchanged.

## Production linear-engine verification

Main now contains a bounded linear engine. Production `.op` and complex `.ac`
results are compared with committed C goldens by variable name, not vector order:
DC uses `1e-12` relative plus `1e-15` absolute near zero; AC uses `1e-10` relative
plus `1e-12` absolute. Analytic RC/RL/RLC, floating-capacitor and coupled-
capacitance transient tests, and opt-in live C Pwl comparisons for the RC,
floating-capacitor and coupled-capacitance decks (`c_linear_reference.rs`, `2e-5` V
on the requested 100 µs grid through the 1 ms/1.01 ms knots), use common physical
output grids rather than identical adaptive internal timesteps. See
[DIFFSOL_FAER_IMPLEMENTATION.md](DIFFSOL_FAER_IMPLEMENTATION.md) for test coverage,
recorded stable/MSRV validation, error bounds and numerical restrictions.

These tests exercise Rust production interfaces, unlike `golden check`, which
only checks reproducibility of captured C output. Topology and rawfile regression
tests remain necessary but do not alone establish simulation correctness.

## Rust-engine golden verification

```sh
cargo xtask golden verify
cargo xtask golden verify --netlist rc_lowpass_ac
```

The default verifies **26 fixtures** through `Parser::parse_file`,
`RunConfig::from_netlist` (so deck `.options`, for example `method=gear`, reach the
driver exactly as in an ordinary run), `Circuit::from_netlist` and the production
analysis runner: `rc_divider`, `rlc_series` and the flattened `subckt_divider`
(`.op`), `rc_lowpass_ac` and `rlc_series_ac` (complex `.ac`), the transients
`rc_transient`, `rl_pulse_tran`, `rc_gear_tran`, `rc_pwl_tran`, `rlc_series_tran`,
`rlc_series_gear_tran`, `floating_cap_tran` and `coupled_cap_tran`, the
initialized-state transients `rc_ic_uic_tran`, `rlc_ic_uic_tran`,
`rc_ic_node_tran` and `floating_cap_ic_tran`, and the M4 nonlinear decks
`diode_dc`, `bjt_ce`, `mos_inverter`, `m4_diode_ac`, `m4_bjt_ac`, `m4_mos1_ac`,
`m4_diode_tran`, `m4_bjt_tran` and `m4_mos1_tran`. It reports **no unsupported
fixtures** — `EXCLUDED` is empty and `subckt_divider` verifies through its own
production path. A requested unsupported
fixture fails, never silently skips. Names are case-insensitive and an optional
`.cir` suffix is accepted. Unknown fixtures/options, missing input/goldens,
unregistered new fixtures and missing default supported decks fail explicitly.
Verify accepts only `--netlist`; it neither locates/executes C nor writes fixtures,
goldens or scratch output. Capture/check/list retain their existing behavior.

The extension registry is `xtask/src/verify.rs` (`SUPPORTED`/`EXCLUDED`). A
`SUPPORTED` entry may list `Variant`s: additional Rust-only runs of the same deck
against the *same* golden whose extra request tokens are appended to the deck's
own (today `backend=diffsol method=bdf`, a syntax C rejects), each with its own
tolerance. Variants never edit decks or goldens, and are refused by `RunConfig` if
the deck selects `method=trap/gear` (no silent downgrade). Add an
analysis kind, axis identity and comparison policy only after demonstrating
production support, not merely parser support. A `SUPPORTED` entry requires
exactly one deck analysis and one C plot; multi-analysis decks are registered in
`BATCH` instead (see "Multi-analysis batch decks (#96)"). `xtask/src/compare.rs` centralizes metadata, shape,
finite-value and numerical checks: plot name/flags, point count, unique variable
names, units and real-vector flags must match. Internal plot IDs and rawfile
Title/Date/Command headers are intentionally not numerical comparisons.
Columns match case-insensitively **by name**, independent of C/Rust ordering.
The frequency axis must be real, positive, strictly increasing and numerically
match at every sample; no interpolation or transient resampling occurs.

Each real/imaginary component satisfies
`|Rust - C| <= relative * |C| + absolute`: DC retains **1e-12 + 1e-15**, and AC
**1e-10 + 1e-12**. The absolute term bounds near-zero currents/cancellation;
relative terms retain the existing production test bounds, not a new looser
policy. Failures report first and worst component mismatches (point, variable,
values, error and bound). Counts explicitly describe bounded coverage, not full
corpus parity.

Ordinary xtask tests cover reordered/renamed/dropped/duplicate columns,
real/imaginary and zero perturbations, metadata/axis/shape/nonfinite errors,
corrupt/missing/multiple goldens, parser/elaboration/numerical failures, missing
registry coverage and process exit statuses. Temporary copies are used for
corruption tests; committed fixtures are never rewritten. Process tests select
an unavailable `NGSPICE_BIN` to verify that C is unnecessary.

## `.option` coverage oracles (#110/#107)

`golden verify` registers `options_gmin_dc` (`.dc`, `compare::NONLINEAR`, junction
`gmin` from a `{}` value) and `options_xmu_tran` (`compare::TRAN`, `xmu=0.2`,
`itl4`), both captured individually (`options_gmin_dc` includes a PNP with
`m=2 area=3`, whose gmin terms scale with `m` only). Opt-in live comparisons in
`tests/c_options_reference.rs` (`NGSPICE_BIN=... cargo test
-p ngspice-rs --test c_options_reference -- --ignored`) cover reverse-biased
diode/NPN/PNP/4-terminal BJT/MOS1 junction `gmin` (`.op`, `.dc`, `.ac`), `xmu`
on a common transient grid, and `{expr}`/`'expr'` `temp`/`tnom` values. Each
numerical test also asserts that the option moves the C result beyond the bound,
so agreement is not vacuous. `iteration_limits_below_c_floor_match_c` asserts
the opposite for `itl1`/`itl2`/`itl4` below 100: C's results are bit-identical
with and without them (`niiter.c` floor), Rust's are bit-identical too, and both
agree (`compare::TRAN` on the `m4_diode_tran` circuit; `.op`/`.dc` at the
Newton tolerances, since two Newton runs stop at different iterates). Deck
`itl1`/`itl2`/`itl4` values *above* 100 are not C-checked: no small deck was
found where C's junction-limited Newton needs more than 100 iterations, so
their effect (and `itl2` as the continuation stage limit) is covered only by
the port-internal tests in `options_coverage.rs`.

## Transient comparison tooling (#48 item 1)

`xtask/src/tran.rs` is the event-aware comparator for the M3 transient exit gate.
All twelve transient fixtures (`rc_transient`, seven gate decks, four initialized-state decks) are
registered in `golden verify` against their committed goldens (see above). The opt-in live comparisons in
`tests/c_companion_reference.rs` cover the PULSE/PWL RC and
series RLC decks for trap and Gear. C rejects `backend=diffsol method=bdf` tokens on `.tran`
("Cannot compute substitute"), so the BDF backend is compared with the committed
goldens only through the registry's Rust-only `Variant`s (same deck text plus
BDF tokens) and otherwise with analytic solutions.

* Timestep sequences are never compared. Both plots are evaluated on a shared
  grid `0, step, ..., stop` (`tran::Grid`); linear interpolation is used only
  between two neighbouring samples of one plot with no breakpoint between them,
  otherwise the comparison fails (it never passes by smoothing over an event).
* Breakpoints come from the deck AST (`tran::breakpoints`): PWL knot times and
  PULSE `TD + n*PER + {0, TR, TR+PW, TR+PW+TF}` with the C `VSRCaccept`
  defaults, never from the data. At a breakpoint the left and right limits are
  compared separately: two samples at one instant are (left, right); one sample
  serves as both limits; no sample at a breakpoint is an error (ngspice lands a
  step on every breakpoint; the companion driver emits one, the diffsol BDF
  requested-grid output only on-grid events).
* `uic` runs: C writes no `t = 0` row (the first row is the first accepted step) and
  adds a breakpoint at the `.tran` step. `tran::Grid` has a `start` (0 normally);
  for a `uic` deck `verify.rs` passes the golden's first time and **both** plots must
  begin exactly there (the instant is compared as an ordinary sample; nothing before
  it is interpolated or extrapolated). The step breakpoint needs no declaration:
  it is not a source corner, both plots carry a sample there, and interpolation
  across it is harmless (the data are smooth).
* End time (and the start) must match `grid.stop`/`grid.start`; missing/extra
  variables, unit/metadata mismatch, non-real data, nonfinite values, decreasing
  time and repeated times away from declared breakpoints are errors.
* `compare::TRAN`: relative 1e-3 (ngspice `reltol`) plus 1e-6 V / 1e-12 A
  (`vntol` / `abstol`) by signal unit. These are the simulator's default accuracy
  floors, not values fitted to a fixture. Every companion (trap/Gear-2) run in the
  registry uses it unchanged; measured worst errors against the committed goldens
  are 0.000 of the bound (the port reproduces C's step sequence).
* `compare::TRAN_RESTART` adds `reltol * max|C signal|` to that bound. It is used
  **only** for Rust-only BDF variants of decks with source corners
  (`rl_pulse_tran`, `rc_pwl_tran`, `rlc_series_tran`, `coupled_cap_tran`). Reason: C restarts its
  trapezoidal rule with a backward-Euler step after every breakpoint (`dctran.c`),
  a first-order local error. Against exact solutions (state-space matrix
  exponential in `m3_gate.rs`, independently RK4) C and the companion driver are off
  by up to 2e-4 (RLC) / 4% of `v(out)` ten microseconds after a PWL corner on a 1 V
  signal, while BDF is exact to ~1e-7. ngspice's own truncation control bounds error
  relative to the peak charge, not the instantaneous value, so a pointwise
  relative bound is stricter than C guarantees at small values. The more accurate
  solver is therefore compared with `reltol` of the signal peak; the worst measured
  ratios are 0.38 (RLC), 0.10 (RC PWL), 0.06 (coupled), 0.05 (RL) of that bound.
  `rl_pulse_tran` passes the pointwise `TRAN` bound too, but at 0.94 of it, so it
  uses the same policy as its siblings. `floating_cap_tran` keeps the pointwise
  `TRAN` bound (0.11).
* Registry design guard: BDF variants require every source corner on the `.tran`
  output grid (the BDF backend emits the requested grid, and the comparator demands
  a sample at each breakpoint) and a stop time that is an exact multiple of `tstep`
  (otherwise the BDF grid ends with a duplicate sample one ulp before the stop,
  which the comparator rightly rejects as a non-breakpoint repeat).

Tests use only synthetic and committed data (no C): grid alignment on unrelated
timesteps, in-segment interpolation, refusal across breakpoints, jump left/right
limits, end-time mismatch, missing signals, nonfinite data, AST breakpoints, and
an end-to-end diffsol BDF RC PWL ramp against its analytic response (worst
error 4e-5 of the bound).

## Initialized-state fixtures (#27, #48)

Four decks with C goldens captured deliberately (one `cargo xtask golden capture
--netlist <name>` each; no existing golden recaptured, ~50-150 KB each) and
registered with `compare::TRAN` unchanged. Worst error against C is 0.000 of the
bound for all four (the port reproduces C's step sequence, including the `uic`
first step and step breakpoint).

| Deck | Initial state | Exercises |
| --- | --- | --- |
| `rc_ic_uic_tran` | `uic`, `c1 ic=2`, 0 V source | RC discharge `2 exp(-t/1 ms)`, no `t = 0` row |
| `rlc_ic_uic_tran` | `uic`, `l1 ic=20m`, `c1 ic=1`, 0 V source | underdamped free decay (zeta 0.158) |
| `rc_ic_node_tran` | `.ic v(out)=0.25`, no `uic`, 1 V source | constrained bias row at `t = 0`, then release |
| `floating_cap_ic_tran` | `uic`, `c1 ic=2` between floating a/b, ramp drive | plate charge changes only by the current through r1 |

The diffsol BDF backend deliberately rejects `.ic`, `uic` and instance `ic=`, so these
decks have **no BDF variants**; `bdf_variants_of_initialized_state_decks_are_rejected_explicitly`
(xtask) and `the_diffsol_bdf_backend_rejects_every_initialized_state_deck_explicitly`
(`m3_gate.rs`) assert the explicit error.

Gate checks in `m3_gate.rs` (Rust and the C golden against the same exact solution
from `t = 0`; budgets at most twice the measurement, relative to the device scale):

| Deck | worst error / scale | budget |
| --- | --- | --- |
| `rc_ic_uic_tran` | 5.9e-6 | 1.2e-5 |
| `rlc_ic_uic_tran` | 2.5e-4 | 4.9e-4 |
| `rc_ic_node_tran` | 2.3e-6 | 4.6e-6 |
| `floating_cap_ic_tran` | 1.5e-6 | 3e-6 |

The floating capacitor's plate charge `C (va - vb)` moves only by the charge through
r1: residual 1.39e-6 of the peak charge (budget 2.8e-6), the first row is within
5.0e-5 of `C ic` (one 0.1 us backward-Euler step of decay; budget 1e-4) and KCL
holds to 1.1e-12 (budget 1e-9). The `.ic` deck's `t = 0` row is `v(out) = 0.25`,
`i(v1) = -0.75 mA`; the same deck without the `.ic` card stays at its 1 V operating
point, so the constraint (not the circuit) set the state, and it is released
afterwards (`v(out) = 1 - 0.75 e^(-t/tau)`). `uic` runs have a first row at
0 < t < tstep/10 equal for Rust and C, a sample at the `.tran` step and the right
breakpoint counts.

## M3 exit-gate analytic and conservation checks (#48)

`tests/m3_gate.rs` runs the committed gate decks through
`Parser` -> `RunConfig` -> `companion_transient`/`runner` (no C needed) and checks
them against exact closed forms (a state-space model advanced with a matrix
exponential over the deck's piecewise-linear drive; the same model judges the
committed C goldens), conservation laws at accepted points, and production-API
semantics. Budgets are measured physical error limits relative to the device scale
(at most 2x the measurement; Rust and C agree to 1e-9 of their own error):

| Deck | method | worst error / scale | budget |
| --- | --- | --- | --- |
| `rl_pulse_tran` | trap | 4.8e-5 | 1e-4 |
| `rc_gear_tran` | Gear-2 | 6.8e-5 | 1.4e-4 |
| `rc_pwl_tran` | trap | 7.1e-6 | 1.5e-5 |
| `rlc_series_tran` | trap | 1.9e-4 | 4e-4 |
| `rlc_series_gear_tran` | Gear-2 | 7.5e-4 | 1.5e-3 |
| `floating_cap_tran` | trap | 1.8e-6 | 4e-6 |
| `coupled_cap_tran` | trap | 3.7e-6 | 8e-6 |
| all of the above | explicit BDF | 1.3e-7 to 3.6e-7 | 7e-7 |

Halving `tmax` on the RLC decks reduces the error by 3.98 then 3.99 (order 2, trap
and Gear). KCL holds to rounding at every accepted point (floating and coupled
networks included). Capacitor charge is conserved to 8e-8 to 1.6e-7 of the peak
plate charge (floating and coupled), energy balance `supplied = stored +
dissipated` to 1.3e-5 to 1.6e-4 of the peak supplied energy; BDF conserves the
floating charge to 2.1e-6 and KCL to 1.6e-8. The `.ac` sweep matches
`H = 1/(1 - w^2 LC + jwRC)`, KCL and KVL to 1e-9. Production-API tests cover: only
accepted points in the output, exact landing on `tstop`, default and explicit
`maxstep`, a sample at every source breakpoint, breakpoint counts, rejected steps
(more rejections at tighter `reltol`, none in the output), work-limit
(`maxsteps`) and minimum-step failures and unsupported `maxord`.

Findings recorded by the gate (no driver change made): deck PWL with a repeated
time is rejected at parse time ("strictly increasing"), so a true source jump is
only reachable through the device API (`companion_transient.rs`); with an extreme
`trtol` on the RLC deck the row-equilibrated companion solves stay well conditioned
and the run ends at the `maxsteps` work limit rather than "timestep too small" (an
explicit failure either way). The BDF requested grid used to end with a duplicate
sample one ulp beside `tstop` when `tstop` was not an exact binary multiple of
`tstep` (e.g. `.tran 50u 3m`); it now ends with exactly one `tstop` sample
(`source_waveforms.rs`).

**Still blocked, not claimed:** higher-index source constraints (#29), nonlinear
charge and devices (M4), integration above order 2 (`dctran.c` never selects
it; `maxord` 3–6 run as 2, see the Gear `maxord` section below) and
nonlinear device initial conditions on the BDF backend (the companion driver
gained them with #99, above), and general MNA DAEs: only the index-one
structures demonstrated above are covered.

## Gear `maxord` 3–6 (#98)

`maxord` 3–6 are accepted for `method=gear` and `method=trap`. ngspice's
`dctran.c` only ever raises the order from 1 to 2 (`CKTmaxOrder > 1`), so the
reference binary writes byte-identical rawfiles for `maxord` 2, 3 and 6 on the
series RLC deck; the port reproduces that policy. Golden
`rlc_series_gear_maxord6_tran` (`method=gear maxord=6`, captured once with
`cargo xtask golden capture --netlist`; no existing golden recaptured, no
tolerance changed; `golden verify` now reports 87 fixtures) verifies under
`compare::TRAN` with worst error 0.000 of the bound, and
`golden_rawfiles.rs::gear_maxord6_golden_is_the_gear2_golden` pins that its C
data equal `rlc_series_gear_tran`'s. Opt-in `c_companion_reference.rs`
(`pulse_series_rlc_with_high_maxord_matches_c`,
`conformance_rlc_gear_maxord6_matches_c`) compares gear and trap `maxord`
3..=6 with live C on a common physical grid: identical point counts, worst
error 1.8e-7 (gear) and 1.7e-7 (trap) of the `1e-3 |C| + 1 uV / 1 pA` bound.

The order 3–6 Gear operations themselves (`maths::integrator`) are checked
without C: exact derivatives and predictions for polynomials of degree `k` on
nonuniform steps (and a detectable error at degree `k + 1`), the fixed-step BDF
tables, agreement with an independent pivoted solve of `nicomcof.c`'s
normalized Vandermonde system, the closed-form `CKTterr` bound
`(trtol tol / max(abstol, c_k |a|))^(1/k)` on degree-`k + 1` polynomials, and
fixed-step convergence on `q' = -q` with a nonuniform repeating step pattern
(error ratios per halving 2.0, 4.0, 8.0-8.1, 16.0-16.3, 32.0-32.7 and 67 for
orders 1-6, asserted within 0.75-1.35 times `2^k`).

## M10 lossless transmission line (#84)

Goldens `m10_tline_tran`, `m10_tline_pulse` and `m10_tline_ac`, captured once
each with `cargo xtask golden capture --netlist <name>` (no existing golden
recaptured, no tolerance changed). The transient decks set the lines'
`rel=1e-3 abs=1e3` so that each line lands on the delayed corners of its waves;
`Gate::Echoes` declares those instants (source corner `b` plus offsets derived
by hand from the deck's `td` values and terminations, documented in the
registry) as breakpoints that both plots must sample and that are never
interpolated across. `golden verify`: `m10_tline_tran` 219 instants + 44
breakpoint limits, `m10_tline_pulse` 271 instants + 100 limits, both worst
error 0.000 of `compare::TRAN`; `m10_tline_ac` 101 points under `compare::AC`.
The C and Rust transients have identical point counts (314 and 1018) and agree
to about 1e-15 V. Internal line nodes (`t1#i1`, `t1#i2`, `t1#int1`,
`t1#int2`) are compared, since C saves them. Analytic, infrastructure and
opt-in live-C coverage is listed in
[TRANSMISSION_LINES.md](TRANSMISSION_LINES.md#verification).

## M10 JFET level 1 (#82)

Four new C goldens, each captured once with `cargo xtask golden capture
--netlist <name>`; no existing golden was recaptured and no tolerance changed.

| Fixture | Gate | Result |
| --- | --- | --- |
| `m10_jfet_dc` | `compare::NONLINEAR` | 231 points (nested `vds` x `vgs` sweep) |
| `m10_jfet_ac` | `compare::NONLINEAR` | 81 points |
| `m10_jfet_tran` | `compare::TRAN` | 493 instants + 16 breakpoint limits, worst 0.033 of bound |
| `m10_jfet_temp` | `compare::NONLINEAR` | 12 points (`.dc temp -40 125 15`) |

Deck design, measured before capture against the same C binary in a scratch
copy:

- **Tight DC tolerances.** With RD/RS and a forward-biased gate, C's default
  Newton stopping test leaves sweep currents up to about 2e-4 relative from
  the converged root (217 times the 1 ppm bound at one point); with
  `reltol=1e-7 vntol=1e-12 abstol=1e-15` both engines agree within 1e-6 of
  the bound's scale, and the port at default tolerances already matches C's
  tightened answer. `abstol=1e-18` (the MOS1 deck's value) makes C's own
  operating point fail, so the JFET decks use `1e-15`.
- **No `off` instance.** `jfetload.c` never clears `icheck` for a held `off`
  load, so C cannot leave `MODEINITFIX` and its operating point fails
  (gmin and source stepping both fail, then garbage values); see
  [JFET.md](JFET.md#newton-start-and-limiting). `off` is covered by Rust tests
  only.
- **Bounded maximum step.** At `tmax = 1 ns` the capacitive gate currents of
  the transient deck (`i(vin)`, `i(vin2)`) differed by up to 2.7 times the
  `TRAN` bound, i.e. by the two drivers' discretization error; at
  `tmax = 0.25 ns` the worst error is 0.033 of the bound.

The opt-in `tests/c_jfet_reference.rs` adds live comparisons of saved
`@j[...]` asks of both polarities, a nested `@j1[area]` x `@j1[temp]` sweep
and `.pz` roots of a common-source stage (agreeing to about 1e-12 relative).

## Model binning verification (#109)

C bins only BSIM3/BSIM4/HiSIM models, which the port does not simulate, so
binning has no rawfile goldens. It is verified at the selection level:

- `src/devices/binning.rs` unit tests: `model_name_match` digit suffixes,
  binnable levels, inclusive 1e-9 m edges, last-declared-wins ordering,
  skipped partial bounds and non-binnable candidates, last-setter precedence,
  the `nf`/`wnflag`/`scale` geometry rule and non-literal errors.
- `tests/model_binning.rs`: the production parser, subcircuit expansion and
  `RunConfig::circuit` path — the selected bin is read from its
  `NotYetPorted` diagnostic, C failure cases are parse errors, subcircuit bin
  sets shadow outer models, root models may not join a local set, and the
  resolver API (`bin_candidates`, `declarations_for`, `select_bin`,
  `with_bin_options`).
- `tests/transistor_parser.rs`: binned `M` references parse (the former
  "MOS model binning" parser gap is gone); `.N`-less and BJT forms do not.
- Opt-in `tests/c_binning_reference.rs`: 21 decks (BSIM3 and BSIM4 bins, bin
  edges and the 1 nm tolerance, reversed declaration order, `m`, missing `w`,
  out-of-range, level-1 bins, mixed level-1/BSIM candidates, partial bounds,
  `nch.01`, an exact model beside bins, local/root/narrow-local subcircuit
  sets) run `op` + `show all : model` in C; the bin C binds (or its "could not
  find a valid modelname") must equal the port's choice, and the sequence of C
  outcomes is pinned so the comparison cannot pass vacuously.

```sh
NGSPICE_BIN=/absolute/path/to/ngspice cargo test -p ngspice-rs --test c_binning_reference --locked -- --ignored
```

Not verified: `.options scale`/`wnflag` and instance `nf`/`wnflag` through a
deck (not accepted by the front end yet; covered only by unit tests), and any
simulation through a binned card.

## Not yet verified

Full corpus simulation, nonlinear D/Q/M arithmetic, trap/Gear transient parity
beyond the linear RC/RLC decks above, general DAEs and the remaining source
waveforms
are not established by the bounded linear implementation; subcircuit elaboration
and parameter scoping are covered by the #18 gate ([SUBCIRCUITS.md](SUBCIRCUITS.md)).
Track those remaining
gates in the central [TODO.md](../../TODO.md); do not claim full SPICE parity.
