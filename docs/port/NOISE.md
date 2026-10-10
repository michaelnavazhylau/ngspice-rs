# `.noise`: small-signal noise analysis (#100)

Implemented for issue #100 (milestone M8) in `src/analysis/noise.rs` (driver)
and `src/devices/noise.rs` (device generator API). C references, read as
behaviour only: `src/spicelib/analysis/noisean.c` (`NOISEan`), `cktnoise.c`
(`CKTnoise`), `nevalsrc.c` (`NevalSrc`, `NevalSrcInstanceTemp`), `ninteg.c`
(`Nintegrate`), `src/maths/ni/niniter.c` (`NInzIter`) and the device routines
`res/resnoise.c`, `dio/dionoise.c`, `bjt/bjtnoise.c`, `mos1/mos1noi.c`,
`sw/swnoise.c`, `csw/cswnoise.c`.

```text
.noise v(out[,ref]) src {dec|oct|lin} pts fstart fstop [pts_per_summary]
```

## Method

1. The operating point is solved exactly as for `.ac`, through the shared
   `analysis::ac::SmallSignal` (`CKTop`, `.nodeset`
   hints, the same Newton/continuation settings and `.option` tolerances), and
   the bias-linearized operators `A` (conductance) and `E` (charge) are
   assembled through `Device::assemble_small_signal` with C's
   `MODEINITSMSIG` state (the zero accepted state, so switches are open, as in
   C).
2. At every frequency `A + j omega E` is factored once (faer complex LU, the
   `.ac` path with its rank and residual guards) and solved twice:
   * **forward**, with a unit excitation of the input source only (C's
     `MODEACNOISE`: every other AC source is zero and the input is driven with
     `1 + 0j` whatever its `ac` value), giving the gain `H` from the input to
     `v(out) - v(ref)`;
   * **transposed** (`ComplexLu::solve_transposed`, C's `SMPcaSolve`), with a
     unit current between the output nodes. Its solution `y` holds every
     node's transfer impedance to the output, so a noise current generator
     between `n1` and `n2` reaches the output with squared gain
     `|y[n1] - y[n2]|^2` (`NevalSrc`).
3. Each generator's output density is `gain * density`, with C's laws:

   | kind | density | C |
   | --- | --- | --- |
   | thermal | `4 k T g` | `THERMNOISE` (`k = 1.38064852e-23`) |
   | shot | `2 q abs(I)` | `SHOTNOISE` (`q = 1.6021766208e-19`) |
   | flicker | `coefficient / f^exponent` | `N_GAIN` times the device's law |

   The output noise density is the sum over every generator; the
   input-referred density divides it by `max(|H|^2, 1e-20)` (`N_MINGAIN`).
4. Each generator is integrated over frequency separately with `Nintegrate`'s
   piecewise power law (flat below a slope of `1e-10`, logarithmic at slope
   `-1`) into the integrated output and input noise.

The frequency loop is C's: `dec`/`oct` multiply by `exp(ln(10 or 2)/pts)` from
`fstart`, `lin` adds `(fstop - fstart)/(pts - 1)`, and the loop continues while
`f <= fstop + tol` with `tol = delta * fstop * reltol` (`dec`/`oct`) or
`delta * reltol` (`lin`), `reltol` being `.option reltol` or C's default `1e-3`.
A one-point `lin` sweep, or any sweep whose start and stop are within 3 ulps
(`AlmostEqualUlps`), measures at `fstart` only.

## Results

Every analysis writes C's plots, in C's order:

| Plot | Title | Columns |
| --- | --- | --- |
| spectrum | `Noise Spectral Density Curves` (real, scale `frequency`) | with `pts_per_summary`: `onoise_<inst><suffix>` per generator and `onoise_<inst>` per instance total, then `onoise_spectrum` and `inoise_spectrum` |
| integrated | `Integrated Noise` (real, one point, no scale), only when `fstart != fstop` | with `pts_per_summary`: `v(onoise_total_<inst><suffix>)`, `v(inoise_total_<inst><suffix>)` pairs; then `v(onoise_total)`, `v(inoise_total)` |

* All values are square roots (V/sqrt(Hz), A/sqrt(Hz), V, A): C without
  `set sqrnoise`, a front-end variable the port has no way to set.
* Units: `onoise_*` are `voltage-density`; `inoise_spectrum` is
  `voltage-density` for a V input and `current-density` for an I input. In the
  integrated plot C's rawfile writer spells the vectors `v(...)`/`i(...)` with
  units `voltage`/`current`; the port uses the same names.
* With `pts_per_summary = n` only every `n`-th frequency (counting the first) is
  written (`step % n == 0`), exactly as C; the integration still uses every
  frequency.
* Instances appear in C's `CKTnoise` order: device types in the `DEVices[]`
  order of `dev.c` (BJT, W, diode, MOS1, R, S), then models in reverse order of
  their creation (a model is created when the first instance naming it is
  parsed; literal resistors share C's default model), then instances in
  reverse deck order. Generator suffixes:

  | Device | Generators (then the instance total) |
  | --- | --- |
  | R | `_thermal`, `_1overf` |
  | D | `_rs`, `_id`, `_1overf`, `_rsw`, `_idsw`, `_1overfsw` |
  | Q | `_rc`, `_rci`, `_rb`, `_re`, `_ic`, `_ib`, `_1overf` |
  | M (MOS1) | `_rd`, `_rs`, `_id`, `_1overf` |
  | S, W | one generator, named `onoise_<inst>`, no separate total |

* A `.noise` card therefore occupies two batch plot names (`noise1 noise2`, or
  one for a single-frequency card): `batch::ScheduledAnalysis::extra_plot_names`,
  `Analysis::run_plots`; `spice-rs simulate` writes both and `golden capture`
  instruments both (`write f.raw noise1.all noise2.all`, whose vector order is
  C's `write` order; the comparators match by name, and the opt-in
  `c_noise_reference` checks the batch `-r` layout itself, order included).

## Device generators

Devices describe their generators at the bias point through
`Device::noise(&NoiseContext) -> SpiceResult<DeviceNoise>`:

```rust
pub enum DeviceNoise {
    Noiseless,
    Sources { family: NoiseFamily, model: Option<String>, total: bool, sources: Vec<NoiseSource> },
}
pub struct NoiseSource { pub suffix: &'static str, pub nodes: [NodeId; 2], pub kind: NoiseKind }
pub enum NoiseKind {
    Thermal { conductance: Real, temperature: Real }, // kelvin
    Shot { current: Real },
    Flicker { coefficient: Real, exponent: Real },
}
```

The device sees only the operating point (`NoiseContext`: model context, node
numbering, bias vector, its small-signal state slots and control rows); the
analysis owns gains, frequencies, integration history and naming. The trait
default is `NotYetPorted`, so a device without a noise port is refused rather
than treated as noiseless. Devices C gives no `DEVnoise` (C, L, V, I, E/F/G/H,
K, B, and the `spice2poly`/`pwl` code models TABLE/POLY lower to) return
`Noiseless` explicitly. That includes RF port sources (`portnum`/`z0`, #105):
C's VSRC has no noise routine, so a port's `z0` termination is noiseless in
`.noise` (verified against C by `c_noise_reference`), and a port can be the
input reference like any V source. `Device::input_source` reports an independent source's
kind and whether its card gave an `ac` value.

| Device | Port of | Generators at the bias |
| --- | --- | --- |
| R (literal and model) | `resnoise.c` | thermal `1/R` (the effective, temperature/scale/`m`-adjusted value) at the instance `temp=` or the circuit temperature; flicker `m KF abs(I/m)^AF / (area f^EF)` with `area = (L - 2 SHORT)^LF (W - 2 NARROW)^WF` when the instance gives `l`/`w`, else 1; `noisy=0` (literal or model instance, IF_INTEGER rounding) removes the resistor |
| D | `dionoise.c` | RS thermal at the instance temperature; shot of the junction current `cd` (gmin current included, as `DIOcurrent`); flicker `KF abs(cd/m)^AF m / f`; the RSW sidewall generators are zero (RSW is not ported) |
| Q | `bjtnoise.c` | RC/RE thermal (`m` included) and RB thermal of the bias-dependent `gx` (RBM/IRB); shot of `cc` and `cb` (`m` included); flicker `m KF abs(cb)^AF / f` between the internal base and emitter; `_rci` zero (no quasi-saturation) |
| M (MOS1) | `mos1noi.c` | RD/RS thermal; channel thermal `2/3 abs(gm)` (NLEV < 3) or the GDSNOI region formula (NLEV 3); flicker NLEV 0 `m KF abs(cd/m)^AF / (f Leff^2 Cox)`, NLEV 1 `... / (f W Leff Cox)`, NLEV 2/3 `KF gm^2 / m / (f^AF W Leff Cox)`, `Cox` for `TOX = 1e-7 m` when the model has none; defaults KF 0, AF 1, NLEV 2, GDSNOI 1 |
| S, W | `swnoise.c`, `cswnoise.c` | thermal of the on or off conductance at the circuit temperature, decided from the small-signal state (any non-zero code is on) |

`KF`/`AF` on D, Q and M models, `KF`/`AF`/`EF`/`LF`/`WF` on R models, MOS1
`NLEV`/`GDSNOI` and the resistor instance `noisy` are accepted model/instance
setters now (they were `NotYetPorted` before #100). MOS1 `NLEV` must round to
0..=3 (C's switch has no other case).

## Verification

* `tests/noise_analysis.rs` (production path, closed forms): resistor-divider
  `4 k T (R1 || R2)` spectra and integrated totals `density * (f2 - f1)`,
  per-instance summaries (names, C order, every-`n`-th sampling, totals), an RC
  low-pass integrating to `kT/C`, resistor KF/AF/EF flicker spectra and the
  analytic `1/f^1.5` integral, diode shot noise against `2 q I`, current-input
  units, differential outputs at `.option temp`, single-frequency sweeps, every
  card/input error, model selector validation, refusal of an unported device,
  and `spice-rs simulate` composing `.noise` with `.ac`/`.op`.
* C goldens (`conformance/netlists/noise_*.cir`, batch fixtures in
  `cargo xtask golden verify`): `noise_rc` (literal/model/flicker/noiseless/
  hot resistors, summary 2; `compare::NOISE`), `noise_diode`, `noise_bjt`
  (current input, tightened RELTOL), `noise_mos1` (every NLEV, PMOS at its own
  temperature) and `noise_multi` (`.ac`, `.op`, a single-frequency and a decade
  `.noise` card, S/W switches) under `compare::NOISE_NONLINEAR`. Both bounds use
  a `1e-20` absolute floor sized for noise values (see `xtask/src/compare.rs`).
* Opt-in live C (`NGSPICE_BIN=... cargo test --test c_noise_reference -- --ignored`):
  the five fixtures plus an octave-swept BJT/diode/MOS1 deck and a PNP stage
  with a current input against ngspice batch mode (`-b -r`), names **in order**,
  units, flags and values (1 ppm, 1e-20 floor).

## Unsupported and divergent

Explicit errors where C would carry on: an output node the circuit does not
have (C creates a floating node), identical output nodes, `fstart <= 0` or
`fstop < fstart`, a sweep type other than `dec`/`oct`/`lin`, fewer than one
point, a negative `pts_per_summary`, an input that is not an independent source
or has no `ac` value, more than 100,000 frequencies, and devices whose noise is
not ported. Not supported: `set sqrnoise` (squared outputs), `.option
keepopinfo`'s extra `NOISE Operating Point` plot, transient noise sources
(`trnoise`/`trrandom`), C's SPICE3-compatibility MOS1 flicker form, noise of
model families the port does not simulate, `.print noise` (the port's `.print`
vectors are node voltages and branch currents, which noise plots do not carry;
such a request fails, as a `.save v(...)` does, where C refuses to run the
analysis). `.measure` cannot target noise plots (rejected by the parser). C's
OP for a high-impedance BJT bias at its default RELTOL differs from the port's
by about 1e-4 relative, which shot noise inherits; the BJT fixture tightens
RELTOL as other nonlinear fixtures do.
