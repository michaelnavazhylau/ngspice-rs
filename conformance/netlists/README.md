# Conformance fixtures

Every `*.cir` file here is a **pure deck**: no `.control` section, exactly one
analysis card, and no file I/O. `cargo xtask golden capture` instruments each one
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

- **One analysis per deck.** `write` writes the *current* plot, so a deck with
  two analyses would need per-plot names. When multi-analysis fixtures are
  wanted, `xtask` should write each plot by name.
- **No `.control` section.** `xtask` refuses to instrument a deck that already
  has one; a fixture's behaviour must come from its `.` cards, not from a
  command script, so that the port has something well-defined to reproduce.
- **No relative includes.** The instrumented deck runs with the scratch
  directory as its working directory, so `.include` paths would not resolve.
  When include support is ported, fixtures that need it should live in
  subdirectories and `xtask` should set the working directory accordingly.

`cargo xtask golden list` prints what each committed golden contains.

| Fixture | Analysis | Exercises |
| --- | --- | --- |
| `rc_divider` | `.op` | two resistors, a voltage source, branch current |
| `rc_lowpass_ac` | `.ac` | complex values, `lin` sweep |
| `rc_transient` | `.tran` | charge storage, timestep control |
| `rlc_series` | `.op` | inductor as a DC short, capacitor as an open |
| `diode_dc` | `.dc` | nonlinear device, source sweep |
| `bjt_ce` | `.op` | BJT with a `.model` card |
| `mos_inverter` | `.op` | MOSFET with instance parameters (`w=`, `l=`) |
| `subckt_divider` | `.op` | `.subckt` / `.ends` and an `X` instance |
