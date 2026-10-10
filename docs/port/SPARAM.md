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

The large-signal `PORT` function adds `sqrt(4 z0 pwr) cos(2 pi freq t)`
to the value left by the previous voltage source in C's reverse-deck load
order (`vsrcload.c`). Consecutive PORTs accumulate, and a nonport V source
with `pwr`/`freq` inherits the previous value with zero amplitude. Without
explicit DC the operating point evaluates this accumulation at zero;
source sweeps propagate changes through the same chain. An explicit DC value
overrides it in DC analyses. Later waveform setters replace PORT; `phase` is
stored but unused, as in C. Both transient backends support PORT. diffsol
requires its inherited baseline to be piecewise linear and retains its
existing index-one DAE restrictions; ordinary SIN/EXP/SFFM/AM still require
the companion backend.

PORT accumulation is checked at full excitation and at source events. DC
continuation retains the port's existing uniform RHS source scaling; C's
non-XSPICE `VSRCload` scales explicit-DC predecessors before adding a no-DC
PORT tone (and applies another scaling in MODETRANOP). Intermediate
continuation paths can therefore differ even though the final full-excitation
equations match; this is not a claim of identical nonlinear branch selection.

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
  `CKTspnoise`/`noisesp.c`) is supported using device noise generators and
  transposed complex solves. Covariances are exported as `i(Cy_i_j)`; two-port
  plots additionally carry `NF`/`NFmin` in dB, complex `SOpt`, and `Rn` in ohms.
  C's RF noise path does not accumulate flicker generators into Cy; this port
  reproduces that behavior. Other `donoise` values disable noise. Undefined
  or nonfinite noise parameters return an explicit numerical error.
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
* `.measure sp` (and `sparam`) uses the frequency axis and named vectors,
  including `mag(S_2_1)`/`real(S_2_1)` components (`MEASURE.md`).
* `.options keepopinfo` retains C's preceding "AC Operating Point" plot,
  including its batch `op` name, for `.ac` and `.sp`.
* Complex sparse LU applies up to two iterative-refinement corrections with
  its existing factors before reporting a failed backward residual (#128).
  The residual threshold and complete-basis rank/conditioning policy are
  unchanged; unresolved numerical systems still fail explicitly.

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

Remaining-M8 checks (#132): `c_sparam_reference` covers hierarchical port
names and retained bias, passive and nonlinear SP noise, and PORT waveforms
against C and analytic values on each backend's physical sample times.
`m8_additional_outputs` exercises SP measurements and print selection through
the CLI; `c_measure_reference` checks measurements against C.
