# S-parameter analysis (`.sp`) and RF port sources (#105)

ngspice's `.sp` is an `RFSPICE` build option. The reference binary used for
verification (`ngspice_test/build`, `ngspice-47+`) is built with `RFSPICE`
(`#define RFSPICE 1` in its `config.h`), so the port is checked against live C
output and committed C goldens, not only against analytic results.

## Port sources

A voltage source becomes an RF port with the instance setters of `vsrc.c`:

```spice
v1 in 0 dc 0 ac 1 portnum 1 z0 50 [pwr 1m] [freq 2.4g] [phase 0]
```

Each setter is `name value` or `name=value` and is applied in deck order, as
`vsrcpar.c` does. Current sources have no port setters (`NotYetPorted` gap).

| Rule (C) | Port behaviour |
| --- | --- |
| `vsrcpar.c` `VSRC_PORTNUM`: `portnum` sets `z0 = 50` when the `z0` set so far is not positive | same, so `z0 0 portnum 1` is a 50 ohm port |
| `INPgetValue(IF_INTEGER)`: `(int) floor(0.5 + value)` | same rounding for `portnum` and the `.sp` `donoise` flag |
| `vsrctemp.c`: a port needs `portnum > 0` and `z0 > 0` (`z0` defaults to 50) | otherwise the source is an ordinary V source; `portnum 1 z0 0` is not a port |
| `vsrctemp.c`: defaults `pwr = 1 mW`, `freq = 1 GHz`, `phase = 0`; `ki = 1/(2 sqrt(z0))` | same (`devices::RfPort`) |
| `vsrctemp.c`: port numbers must be exactly `1..=N` (fatal "incorrect port ordering" / "duplicate port Index") for **every** analysis | `Circuit::finalize` rejects the deck (exit 2) |
| `vsrcset.c`: a port creates the internal node `<name>#res` | same node, which C saves and the port plots (`v(v1#res)`) |
| `vsrcload.c`/`vsrcacld.c`: the ideal source sits between `#res` and the negative terminal; `1/z0` is stamped between the positive terminal and `#res` in **every** analysis | same in `.op`, `.dc`, `.ac`, `.tran` (both backends) and `.sp`: a port is a Thevenin source with series `z0`, and `i(v1)` is the current through the ideal source |

The large-signal `PORT` time function selected by `pwr`/`freq` is only
partly ported: `vsrcload.c` adds `sqrt(4 z0 pwr) cos(2 pi freq t)` to the
value left over from the previously loaded source instance, which the port
does not reproduce. Therefore:

* with an explicit DC value, DC and small-signal analyses (`.op`, `.dc`, `.ac`,
  `.sp`) run normally (C uses the DC value and the AC phasor there);
* any transient with such a port is `NotYetPorted` (companion and diffsol
  backends), unless a later waveform setter (`sin(...)`, `pulse(...)`, …)
  replaced the `PORT` function, as it does in `vsrcpar.c`;
* a port with `pwr`/`freq` and no DC value is `NotYetPorted` (C loads the
  `PORT` function even in the operating point), and so is `pwr`/`freq` on a
  source that is not a port;
* `phase` is stored and has no effect, as in C.

## `.sp` analysis

```spice
.sp {lin|dec|oct} points fstart fstop [donoise]
```

`span.c` (`SPan`), `cktspdum.c` and `vsrcacld.c` (`VSRCspinit`,
`VSRCspupdate`), ported in `analysis::sparam`:

1. The operating point, nodesets, `hertz` re-solves and the frequency grid are
   exactly those of `.ac` (`analysis::ac::SmallSignal`, `frequency_grid`); the
   deck's DC options reach `.sp` as they reach `.ac`.
2. In `MODESP` no independent source excites the circuit: `VSRCacLoad` loads
   zero for every V source. At each frequency port `j` in turn drives a unit
   voltage into its ideal-source branch while every port keeps its `z0`.
3. For every port `i`: `V` = positive minus negative terminal voltage, `I` =
   minus the branch current (into the positive terminal),
   `a = ki (V + z0 I)`, `b = ki (V - z0 I)` (`CKTspCalcPowerWave`), giving
   column `j` of `A` and `B`.
4. `S = B A^-1`; `Z = Gn^-1 (E - S)^-1 (S Z0 + Z0) Gn`;
   `Y = Gn^-1 (S Z0 + Z0)^-1 (E - S) Gn`, `Z0 = diag(z0)`, `Gn = diag(2 ki)`
   (`CKTspCalcSMatrix`). These are `N x N` port matrices
   (`maths::dense_complex`); the circuit solves use the faer complex sparse LU
   of `.ac`, one factorization per frequency and one solve per port.

With several ports with different `z0`, S is the power-wave S matrix
`S_ij = sqrt(z0_j / z0_i) [(Z - Z0)(Z + Z0)^-1]_ij`.

### Output

One complex plot, `Plotname: SP Analysis`, in-memory name `sp<n>`, run after
every other analysis type (`SPinfo` follows `SENSinfo` in `analInfo[]`):

| Vectors | Type | Content |
| --- | --- | --- |
| `frequency` | `frequency` | the grid |
| `v(...)`, `i(...)` | `voltage`, `current` | node voltages (including `#res`) and branch currents of the **last** port's excitation (C dumps `CKTrhsOld` after the final solve) |
| `S_i_j`, `Y_i_j`, `Z_i_j` | `s-param`, `admittance`, `impedance` | row `i` = response port, column `j` = driven port, row-major |
| `v(Rbase)` | `voltage` | port 1's `z0` (C's Touchstone reference) |

The spellings are those of `ngspice -b -r`; C's `.control` `write` lowercases
them (`s_1_1`, `v(rbase)`), which the goldens keep. Comparisons are by name,
case-insensitively. As for `.ac`, the port does not write C's `grid=3`
attribute for `dec`/`oct` scales.

## Divergences and unsupported cases

* **`donoise`** (`.sp ... 1`: `Cy_i_j`, `NF`, `SOpt`, `NFmin`, `Rn` from
  `CKTspnoise`/`noisesp.c`) is `NotYetPorted`. Other `donoise` values are C's
  "no noise" and run normally.
* **AC current sources.** `span.c` saves the AC-load RHS before the port loop
  with `memcpy(rhswoPorts, CKTrhs)` followed by `memcpy(rhswoPorts, CKTirhs)`
  and an all-zero imaginary copy, so a current source's *imaginary* AC phasor
  becomes a real excitation in every port solve and changes C's
  S-parameters (verified against the reference binary); its real phasor is
  dropped. The port refuses a `.sp` deck with a current source of nonzero
  imaginary AC phasor (`Unsupported`) and, like C, ignores purely real ones.
* **Nonexistent Y or Z.** For a series element Z does not exist (`E - S` is
  singular), for a shunt element Y does not. C's `cinverse` zero-fills only an
  *exactly* singular matrix; with rounding-level pivots it returns huge,
  rounding-dependent values (`-2.25e17` for a 50 ohm series resistor). The port
  detects singularity to working precision (pivot at most `128 n eps` times the
  largest entry) and writes the zero block, C's singular contract. S is always
  defined (`A` is diagonal for Thevenin ports); a singular `A` is a numerical
  error.
* **No RF port** is an error (C: "No RF Port is present, cannot run sp
  analysis" and `controlled_exit`).
* `.measure sp` (and `sparam`) is `NotYetPorted` (`MEASURE.md`).
* C's `keepopinfo` "AC Operating Point" plot is not produced (as for `.ac`).
* The `.ac`-path complex sparse LU's backward-residual guard can refuse some
  reactive decks at particular frequencies (for example an LC ladder with
  50 ohm terminations between 280 and 490 kHz); this affects `.ac` and `.sp`
  alike and is not specific to S-parameters.

## Verification

* `tests/sparam.rs`: analytic series and shunt resistors (with the zero Z/Y
  block), matched T attenuators for three `(z0, K)` pairs (S11 = 0,
  S21 = 1/K, closed-form Z and Y), a lossy RC network between 50 and 75 ohm
  ports against closed-form Z/Y/S and reciprocity, a one-port RC load,
  port-number ordering, MODESP source switching, ports in `.op`/`.ac`/`.tran`,
  batch order and plot names, setter order/defaults, numbering errors and every
  rejection above; `maths::dense_complex` unit tests (pivoting, singularity).
* C goldens (`cargo xtask golden capture --netlist <name>`, one fixture at a
  time; no existing golden recaptured), verified by `cargo xtask golden verify`
  under `compare::AC` (and `compare::DC`/`compare::TRAN` for the other
  `sp_multi` plots): `sp_attenuator`, `sp_rc` (single-analysis registry) and
  `sp_multi` (`.ac .op .tran .sp`, batch registry). The verify projection keeps
  `#res` nodes, which C's `outitf.c` saves.
* `tests/c_sparam_reference.rs` (opt-in, `NGSPICE_BIN`): C batch mode
  (`ngspice -b -r`) against `spice-rs simulate` on the three fixtures, one- and
  three-port decks, a reordered unequal-`z0` deck, an `oct` grid, a 5-pole
  Chebyshev LC low-pass, a `pwr`/`freq` port deck and a port transient; every
  value by name within `1e-9 |C| + 1e-12` (the `c_batch_reference.rs`
  policy), and the series-resistor deck documents the Z divergence.
