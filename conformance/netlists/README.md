# Conformance fixtures

Every `*.cir` file here is a **pure deck**: no `.control` section and no file
I/O. Most have exactly one analysis card; `multi_analysis_rc`, the M6 exit
gate `m6_gate` and the RF-port deck `sp_multi` deliberately have four (see below). `cargo xtask golden capture` instruments each one
by inserting

```spice
.control
set filetype=ascii
run
write <fixture>.raw
.endc
```

immediately before the first `.end` card, runs the C `ngspice` binary on it, and
stores the resulting ASCII rawfile in `../golden/<fixture>.raw`.

Why these rules:

- **One analysis per deck, unless multi-analysis is the point.** `write` writes
  the *current* plot, so a deck with several analyses is instrumented to write
  each plot by its C name (`write <fixture>.raw ac1.all dc1.all ...`, in batch
  order, then `quit`) and is registered in the `BATCH` verification registry
  (#96).
- **No `.control` section.** `xtask` refuses to instrument a deck that already
  has one; a fixture's behaviour must come from its `.` cards, not from a
  command script, so that the port has something well-defined to reproduce.
- **No relative includes.** The instrumented deck runs with the scratch
  directory as its working directory, so `.include` paths would not resolve.
  When include support is ported, fixtures that need it should live in
  subdirectories and `xtask` should set the working directory accordingly.

`cargo xtask golden list` prints what each committed golden contains.

The eight M3 gate decks (`rl_pulse_tran` ... `rlc_series_ac`, GitHub #48) were captured
with `cargo xtask golden capture --netlist <name>` (one fixture at a time, so no
existing golden is touched). Design rules they follow: sample spacing (`tstep`,
which is also the default maximum step) is small against the circuit time
constants and source edges so that comparing resampled waveforms measures
integration error, not interpolation error; source corners of the decks that also
run on the explicit BDF backend lie on the `.tran` output grid and the stop time
is an exact multiple of `tstep`; `.ac` uses `lin` because the Rust rawfile writer
does not reproduce ngspice's `grid=` header attribute for `dec`/`oct`. The four initialized-state decks (`rc_ic_uic_tran` ... `floating_cap_ic_tran`, GitHub
#27/#48) were captured the same way; they have no BDF variants because the diffsol
backend rejects `.ic`/`uic`/`ic=`. With `uic` C writes no `t = 0` row and adds a
breakpoint at the `.tran` step; the comparator starts at the first common sample.

| Fixture | Analysis | Exercises |
| --- | --- | --- |
| `rc_divider` | `.op` | two resistors, a voltage source, branch current |
| `rc_lowpass_ac` | `.ac` | complex values, `lin` sweep |
| `rc_transient` | `.tran` | charge storage, timestep control |
| `rlc_series` | `.op` | inductor as a DC short, capacitor as an open |
| `rl_pulse_tran` | `.tran` | PULSE-driven R-L (tau = 100 us), inductor flux, trapezoidal |
| `rc_gear_tran` | `.tran` | `.options method=gear` (Gear-2) on a PULSE RC |
| `rc_pwl_tran` | `.tran` | PWL drive with corners on the output grid |
| `rlc_series_tran` | `.tran` | underdamped series RLC (zeta = 0.158), PULSE, trapezoidal |
| `rlc_series_gear_tran` | `.tran` | same circuit, Gear-2 |
| `rlc_series_gear_maxord6_tran` | `.tran` | same circuit, `method=gear maxord=6` (#98): `dctran.c` never raises the order above 2, so C's data equal `rlc_series_gear_tran` |
| `floating_cap_tran` | `.tran` | floating capacitor between two resistive nodes (rank-deficient mass, index one) |
| `coupled_cap_tran` | `.tran` | coupled capacitances (nondiagonal, nonsingular mass block) |
| `rlc_series_ac` | `.ac` | complex RLC low-pass sweep through resonance (`lin`) |
| `rc_ic_uic_tran` | `.tran ... uic` | RC discharge from capacitor `ic=2`, no `t = 0` row |
| `rlc_ic_uic_tran` | `.tran ... uic` | series RLC free decay from inductor `ic=20m` and capacitor `ic=1` |
| `rc_ic_node_tran` | `.tran` + `.ic v(out)=0.25` | `.ic` enforced in the initial bias only (no `uic`), then released |
| `floating_cap_ic_tran` | `.tran ... uic` | floating capacitor with initial plate charge (`ic=2`), ramp drive |
| `rc_sin_tran` | `.tran` | delayed, damped, phase-shifted SIN (#94); constant PWL marker lands C on `TD` |
| `rc_exp_tran` | `.tran` | EXP rise and fall (#94); constant PWL marker lands C on `TD1`/`TD2` |
| `rc_sffm_am_tran` | `.tran` | SFFM voltage source and AM current source (#94), continuous at `t = 0` |
| `rc_pwl_repeat_tran` | `.tran` | PWL `r=0 td=0.2m` triangle (#95), repeated knots are C breakpoints |
| `rc_pulse_count_tran` | `.tran` | PULSE eighth field `NP = 3` (#95): three pulses, then V1 |
| `diode_dc` | `.dc` | nonlinear device, source sweep |
| `bjt_ce` | `.op` | BJT with a `.model` card |
| `mos_inverter` | `.op` | MOSFET with instance parameters (`w=`, `l=`) |
| `subckt_divider` | `.op` | `.subckt` / `.ends` and an `X` instance |
| `func_quotes` | `.op` | `.func` (top-level and body-local, free names resolved at the call site) and single-quoted values (#107) |
| `m4_diode_ac` | `.ac` | bias-linearized diode RS/CJO/TT and internal anode |
| `m4_diode_tran` | `.tran` | junction depletion/diffusion charge, PULSE and KCL |
| `m4_bjt_ac` | `.ac` | level-1 NPN forward/reverse transport, CJE/CJC/TF |
| `m4_bjt_tran` | `.tran` | independent BE/BC charge companions |
| `m4_mos1_ac` | `.ac` | MOS1 square law, body junction and overlap charges, zero TOX |
| `m4_mos1_tran` | `.tran` | five MOS1 charge pairs and pulse bias |
| `m7_zener_dc` | `.dc` | Zener regulator from forward conduction through BV/IBV/NBV breakdown, `.options reltol=1e-6 vntol=1e-9` (#86) |
| `m7_diode_physics_dc` | `.dc` | ISR/NR recombination, JTUN/JTUNSW tunnelling, NS sidewall with breakdown, IKF/IKR/IKP knees (#86) |
| `m7_diode_temp_dc` | `.dc temp` | EG/XTI/TNOM, TLEV 2 band gap, DTEMP, TRS and TCV breakdown across -40..125 C (#86) |
| `m7_diode_temp_ac` | `.ac` | `.options temp=100`: TLEVC 0/1 depletion laws, CJSW/PJ, ISR small signal, TTT1 (#86) |
| `m7_zener_tran` | `.tran` | SIN-driven Zener clipper: breakdown, junction/sidewall/diffusion charge, `reltol=1e-5` (#86) |
| `m7_mos1_inverter_tran` | `.tran` | CMOS inverter: Meyer gate charge (TOX), LD, RSH/NRD/NRS and RD/RS internal nodes, CJ/CJSW/JS geometry; `tmax` 2 ps (#88) |
| `m7_mos1_ring_tran` | `.tran` | 3-stage CMOS ring oscillator with 50 fF loads and a current kick; `tmax` 0.5 ps (#88) |
| `m7_mos1_meyer_ac` | `.ac` | Meyer and junction small-signal capacitance at 75 C, `M=2`, LD, RD/RS (#88) |
| `m7_mos1_process_dc` | `.dc` | NSUB/TPG/NSS/UO extraction, forward body bias, TEMP/DTEMP/TNOM, PMOS RSH; tight RELTOL (#88) |
| `options_gmin_dc` | `.dc` | `.options gmin={gj}` (from `.param`) on reverse diode/PNP junctions, a PNP with `m=2 area=3` (gmin scales with `m` only), `itl1`/`itl2`, documented no-op options |
| `options_xmu_tran` | `.tran` | `.options xmu=0.2 itl4=20` on a PULSE RC (trapezoidal weighting; `itl4=20` is C's effective 100) |
| `multi_analysis_rc` | `.tran` `.ac` `.op` `.dc` | four analyses in one deck: C batch order (`.ac .dc .op .tran`), one plot each in a single rawfile (#96) |
| `m6_gate` | `.tran` `.ac` `.op` `.dc` | M6 exit gate: SIN-driven K transformer (1:2, k = 0.98) into a G/E op-amp subcircuit (gain `{gain}` from `.param`), `.func` B limiter, H current sense, `.option reltol=1e-4` |
| `controlled_op` | `.op` | E/F/G/H signs, E op-amp loop (gain 1e4), F sensing an E branch, H inside a subcircuit, HSPICE keyword, `(a,b)` controls, G `m=` |
| `controlled_ac` | `.ac` | E integrator (gain 1e4), G into an RC, F/H sensing a load current (`lin`) |
| `controlled_tran` | `.tran` | PULSE RC buffered by E, G charging a second RC, F/H sensing its current |
| `switch_op` | `.op` | S/W hysteresis bands decided by ON/OFF, controls outside the band, default RON/ROFF (gmin), W sensing a V branch, a W latch (#81) |
| `switch_dc` | `.dc` | downward sweep through S/W bands (positive, negative and zero hysteresis): accepted switch state carried from point to point |
| `switch_tran` | `.tran` | PULSE-controlled charge sharing, SIN-controlled discharge, a self-controlled relaxation oscillator (`swtrunc.c` step control) |
| `switch_w_tran` | `.tran` | W switches sensing a SIN and a PULSE current (positive and negative hysteresis, ON flag) |
| `switch_dc_decimal` | `.dc` | 0.1 V step: C's accumulated sweep values (`0.9999999999999999`, `1.5000000000000002`) decide S and W at their thresholds; ON/OFF flags at the first point |
| `switch_ac` | `.ac` | C's `MODEINITSMSIG` switch state: every switch open in AC, including ones closed at the operating point |
| `bsource_op` | `.op` | B sources: the `inpptree.c` functions, comparisons/logic/ternary, `.param`, `.func`, `m`/`tc1`/`tc2`/`temp`, `i(b1)` through the inserted `v_b1` |
| `bsource_dc` | `.dc` | nonlinear B transfer curves (exponential current, tanh limiter, `pwl()`, power laws, ternary) |
| `bsource_ac` | `.ac` | B sources linearised at the bias point, including a sensed source current |
| `bsource_tran` | `.tran` | `time` in `sin()`/`pwl()`/`exp()` (no breakpoints, 1 us maximum step) driving an RC with nonlinear B loads |
| `evalue_op` | `.op` | E `VALUE=`/`VOL=`, G `VALUE=`/`CUR=` with `m=`, a VALUE E inside a subcircuit, `i(e2)` sensing |
| `gtable_dc` | `.dc` | E/G `TABLE` (XSPICE `pwl` map, `* xtask-codemodels: analog`), single-pair and LTspice four-node forms |
| `epoly_dc` | `.dc` | E/G/F/H `POLY(n)` up to three dimensions and the implicit `POLY(1)` (`* xtask-codemodels: spice2poly`) |
| `bsource_zero_op` | `.op` | `1/x`, `sqrt`, `log`/`ln`/`log10` and divisions whose controlling nodes start Newton at 0 V (`1e32` slopes, `log(0) = -1e99`) |
| `bsource_zero_dc` | `.dc` | the same singular slopes at the first sweep point's 0 V start |
| `bsource_zero_tran` | `.tran` | the same at the initial operating point, then a 1–3 V `sin` input into resistive loads (1 us maximum step) |
| `m7_bjt_gummel` | `.dc` | Gummel plot (VBC = 0) of a Gummel-Poon NPN: VAF/VAR, IKF/IKR, ISE/ISC leakage, RB/RBM/IRB, RC/RE internal nodes (#87) |
| `m7_bjt_output` | `.dc` | nested output characteristics, VCE inner and IB outer (C writes no outer column) |
| `m7_bjt_temp` | `.dc` | `temp` sweep -40..125 C: NPN (XTB/XTI/EG), lateral PNP with substrate ISS, TLEV=1 and polynomial tempcos, TNOM; TLEV=3/TLEVC=1 NPN with IBE/IBC, NKF, `dtemp`/`area`/`areab`/`m` |
| `m7_bjt_amp_ac` | `.ac` | CE amplifier at 50 C: CJE/CJC with XCJC, CJS substrate (4th terminal), TF with XTF/VTF/ITF, TR |
| `m7_bjt_amp_tran` | `.tran` | the same amplifier driven by a 10 mV PULSE: every charge companion |
| `m7_conv_latch_op` | `.op` | cross-coupled BJT latch (#106): ngspice's MODEINITJCT start, `DEVpnjlim` and `dynamic_gmin` settle on the metastable point, `.option reltol=1e-8` |
| `m7_conv_latch_gillespie_op` | `.op` | the same latch with `.options noopiter gminsteps=0 srcsteps=1`: `gillespie_src` only |
| `m7_conv_latch_spice3_gmin_op` | `.op` | the same latch with `noopiter gminsteps=4`: `spice3_gmin` |
| `m7_conv_latch_spice3_src_op` | `.op` | the same latch with `noopiter gminsteps=0 srcsteps=4`: `spice3_src`, which lands in a stable state |
| `m7_conv_latch_tran` | `.tran` | a cross-coupled pair switched by set/reset PULSEs through junction charges (10 ns maximum step, `reltol=1e-7`) |
| `m7_conv_bjt_schmitt` | `.dc` `.dc` `.op` | emitter-coupled BJT Schmitt trigger swept up and down through its hysteresis (warm-started `.dc` points) and an `.op` inside the band |
| `m7_conv_cmos_schmitt` | `.dc` `.dc` `.op` | six-transistor MOS1 Schmitt trigger swept up and down and an `.op` inside the band (`reltol=1e-8 vntol=1e-12`) |
| `m7_ic_diode_uic_tran` | `.tran` | `uic` diode/RC discharge (#99): capacitor `ic=2`, the other capacitor and the diode junction (CJO, TT, RS) from `.ic v(b)=0.5`; the diode's own `ic=0.4` has no effect in C (`reltol=1e-5`) |
| `m7_ic_bjt_flipflop_tran` | `.tran` | symmetric BJT flip-flop whose `.ic` node voltages select the state through the forced transient operating point; a reset pulse flips it |
| `m7_ic_bjt_off_tran` | `.tran` | BJT flip-flop with an `OFF` transistor: C's MODEINITFIX hold and device convergence test hand the operating point to dynamic gmin, which lands in a stable state; a negative pulse flips it (`reltol=1e-6`) |
| `m7_ic_mos1_uic_tran` | `.tran` | MOS1 inverter pair started under `uic` from full and partial `ic=` vectors and the `.ic` node vector (Gear-2, `reltol=1e-5`, 2 ps maximum step) |
| `m7_ic_latch_nodeset_op` | `.op` | symmetric CMOS latch whose `.nodeset` (forced in MODEINITJCT/MODEINITFIX only) selects the q-high state |
| `m7_ic_latch_mos1_ic_op` | `.op` | the same latch whose MOS1 `ic=` vectors move the MODEINITJCT start (no `uic`) and select the q-high state |
| `m8_tf_divider` | `.tf` | V-driven ladder, `v(out)`: gain and input/output resistance with an inductor short and a capacitor open (#101) |
| `m8_tf_controlled` | `.tf` | current-source input, E/G amplifier, `i(vs)` output through a zero-volt sense source (#101) |
| `m8_tf_bjt` | `.tf` | Gummel-Poon CE stage with bias-dependent base resistance, linearised with C's `gx`-only matrix (`reltol=1e-8`, #101) |
| `m8_tf_batch` | `.op` + 2 `.tf` | diode `v(d,dm)` and MOS1 `i(vdd)` transfer functions in ngspice batch order (`op1 tf1 tf2`, #101) |
| `sp_attenuator` | `.sp` | matched K = 3 T pad between 50 ohm ports: S11 = 0, S21 = 1/3, closed-form Y/Z, port `#res` nodes, `v(rbase)` (#105) |
| `sp_rc` | `.sp` | lossy RC two-port between 50 and 75 ohm ports: unequal power-wave normalisation, reciprocal S12 = S21, complex Y/Z (#105) |
| `sp_multi` | `.sp` `.tran` `.ac` `.op` | RF ports in every analysis (series z0), a later `sin()` overriding `pwr`, batch order `.ac .op .tran .sp` (#105) |
| `pz_ladder_cur` | `.pz` | current-driven RLC ladder read on the negative output node (C swaps the drive): a real pole and a complex pair |
| `pz_bridge_diff` | `.pz` | RC bridge with a differential output (C's column addition) and a zero at the origin |
| `pz_transformer` | `.pz` | K-coupled transformer into an RC load: complex pair, real pole, zero at the origin |
| `pz_cv_loop` | `.pz` | decoupling capacitor across an ideal supply (an index-two block that must add no pole) |
| `pz_diode` | `.pz` | forward-biased diode with `rs`, depletion and diffusion charge, linearized at the operating point |
| `pz_mos1` | `.pz` | MOS1 stage with current drive at a floating gate: a pole at the origin and a right-half-plane zero |
| `multi_analysis_pz` | `.ac`, `.op`, two `.pz` | batch order and plot names with pole-zero plots (`pz1` zeros, `pz2` poles) |

The six M4 decks were individually captured with the existing ngspice-47+ build;
previous goldens were not recaptured. See [M4_NONLINEAR.md](../../docs/port/M4_NONLINEAR.md)
for the demonstrated local #41 gate, physics allowlists and justified tolerances.
Only the subcircuit fixture remains excluded by Rust-engine verification.

The five M6 source decks (`rc_sin_tran` ... `rc_pulse_count_tran`, #94/#95) were
captured individually the same way; no existing golden was recaptured.

The two `options_*` decks (#110/#107) were captured one at a time with
`cargo xtask golden capture --netlist <name>`; no existing golden was touched.

The three `controlled_*` decks (#78) were captured one at a time with
`cargo xtask golden capture --netlist <name>`; no existing golden was touched.
See [CONTROLLED_SOURCES.md](../../docs/port/CONTROLLED_SOURCES.md).

The six `switch_*` decks (#81) were captured one at a time with
`cargo xtask golden capture --netlist <name>`; no existing golden was touched.
In the two transient decks every plotted node is a source or capacitor node,
so no plotted value jumps between samples where a switch flips.

The ten behavioural-source decks (#79) were captured one at a time with
`cargo xtask golden capture --netlist <name>`; no existing golden was touched.
`gtable_dc` and `epoly_dc` need XSPICE code models, which the capture loads
through a scratch `.spiceinit` because their decks carry a
`* xtask-codemodels:` comment. See
[BEHAVIOURAL_SOURCES.md](../../docs/port/BEHAVIOURAL_SOURCES.md).

The five M7 diode decks (#86) were captured one at a time with
`cargo xtask golden capture --netlist <name>`; no existing golden was touched.
See [M4_NONLINEAR.md](../../docs/port/M4_NONLINEAR.md#diode).

The five `m7_bjt_*` decks (#87) were captured one at a time with
`cargo xtask golden capture --netlist <name>`; no existing golden was touched.
Each sets `.option reltol=1e-8`: at C's default `reltol` (with bypass) the
Gummel-Poon sweeps stop about 4e-4 away from the converged root, outside the
1 ppm nonlinear bound; see [M4_NONLINEAR.md](../../docs/port/M4_NONLINEAR.md).

The seven `m7_conv_*` convergence decks (#106) were captured one at a time with
`cargo xtask golden capture --netlist <name>`; no existing golden was touched.

The six `m7_ic_*` nonlinear initial-condition decks (#99) were captured one at
a time with `cargo xtask golden capture --netlist <name>`; no existing golden
was touched. Where a deck sets `reltol` (and Gear-2 for the MOS1 deck), C's
own answer at its default tolerance is outside, or too close to, the
transient bound: at the default `reltol` the diode deck's C `v(b)` at 0.1 us
is 5.1e-3 V (about 4.6 times the bound) away from its own `reltol=1e-5`
answer, the OFF flip-flop compares at 0.53 of the bound through its flip, and
the MOS1 deck's trapezoidal gate currents ring from step to step on the flat
input. The `.ic` flip-flop keeps every default. See
[VERIFICATION.md](../../docs/port/VERIFICATION.md#m7-nonlinear-initial-conditions-99).

The seven pole-zero decks (#103) were captured one at a time with
`cargo xtask golden capture --netlist <name>`; no existing golden was touched.
Each has at least two roots (C's `write` adds a copy named `all` to a plot with a
single vector) and is one on which C's root search finishes without a warning;
`golden verify` compares the roots as unordered sets
([POLE_ZERO_ADR.md](../../docs/port/POLE_ZERO_ADR.md)).
