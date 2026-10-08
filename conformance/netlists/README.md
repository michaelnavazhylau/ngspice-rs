# Conformance fixtures

Every `*.cir` file here is a **pure deck**: no `.control` section and no file
I/O. Most have exactly one analysis card; `multi_analysis_rc` deliberately has
four (see below). `cargo xtask golden capture` instruments each one
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
| `floating_cap_tran` | `.tran` | floating capacitor between two resistive nodes (rank-deficient mass, index one) |
| `coupled_cap_tran` | `.tran` | coupled capacitances (nondiagonal, nonsingular mass block) |
| `rlc_series_ac` | `.ac` | complex RLC low-pass sweep through resonance (`lin`) |
| `rc_ic_uic_tran` | `.tran ... uic` | RC discharge from capacitor `ic=2`, no `t = 0` row |
| `rlc_ic_uic_tran` | `.tran ... uic` | series RLC free decay from inductor `ic=20m` and capacitor `ic=1` |
| `rc_ic_node_tran` | `.tran` + `.ic v(out)=0.25` | `.ic` enforced in the initial bias only (no `uic`), then released |
| `floating_cap_ic_tran` | `.tran ... uic` | floating capacitor with initial plate charge (`ic=2`), ramp drive |
| `diode_dc` | `.dc` | nonlinear device, source sweep |
| `bjt_ce` | `.op` | BJT with a `.model` card |
| `mos_inverter` | `.op` | MOSFET with instance parameters (`w=`, `l=`) |
| `subckt_divider` | `.op` | `.subckt` / `.ends` and an `X` instance |
| `m4_diode_ac` | `.ac` | bias-linearized diode RS/CJO/TT and internal anode |
| `m4_diode_tran` | `.tran` | junction depletion/diffusion charge, PULSE and KCL |
| `m4_bjt_ac` | `.ac` | level-1 NPN forward/reverse transport, CJE/CJC/TF |
| `m4_bjt_tran` | `.tran` | independent BE/BC charge companions |
| `m4_mos1_ac` | `.ac` | MOS1 square law, body junction and overlap charges, zero TOX |
| `m4_mos1_tran` | `.tran` | five MOS1 charge pairs and pulse bias |
| `multi_analysis_rc` | `.tran` `.ac` `.op` `.dc` | four analyses in one deck: C batch order (`.ac .dc .op .tran`), one plot each in a single rawfile (#96) |

The six M4 decks were individually captured with the existing ngspice-47+ build;
previous goldens were not recaptured. See [M4_NONLINEAR.md](../../docs/port/M4_NONLINEAR.md)
for the demonstrated local #41 gate, physics allowlists and justified tolerances.
Only the subcircuit fixture remains excluded by Rust-engine verification.
