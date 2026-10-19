# Linux x86-64 GNU C reference

Only `sens_bjt.raw` needs this platform variant. The original
`../sens_bjt.raw` is the unchanged macOS arm64 capture. All other fixtures
continue to use their original files.

## Provenance

- Source: pristine upstream ngspice revision
  `059066b532f7073fccf4173ef269309eafdc7f0c` (`ngspice-47+`), the same
  reference checkout used for the macOS fixtures.
- Platform: Debian bookworm, Linux x86-64 (Docker `--platform linux/amd64`).
- GCC: `12.2.0-14+deb12u1`; glibc: `2.36-9+deb12u13`.
- C flags: `-O2 -ffp-contract=off`.
- Configure: `--with-x=no --with-readline=no --enable-xspice --enable-cider`.
- Capture: the production `cargo xtask golden capture --netlist sens_bjt
  --ngspice /absolute/path/to/linux/ngspice` on this platform. The ASCII file
  is C's unedited `write` output, not Rust output or a converted binary file.

The C source was archived into an isolated container build directory. No
upstream C checkout, existing golden, device equation, solver policy or
comparison tolerance was changed.

## Why a separate reference is necessary

`bjtsetup.c` scales the card's `ISE=1e-3` by `IS=1e-16`, leaving `1e-19`.
`cktsens.c` then perturbs it by `1e-6 * ISE = 1e-25`. Platform rounding of
the load subtraction is amplified by that divisor. The same C source gives:

| Column | macOS arm64 C | Linux x86-64 C |
| --- | ---: | ---: |
| `q1:ise` | 2.867749531079332e10 | -1.5951873897539978e10 |
| `q1:irb` | 3.457080497929780e-6 | -1.0058559802750485e-6 |
| `q1_area` | 3.014364737225222e-6 | 3.0121332688217968e-6 |

The ISE discrepancy corresponds to only `4.46e-15` in the solved response.
Rust reproduces Linux C; comparing it to the macOS capture caused PR #183's
CI failures. This is not evidence that the huge ISE number is a portable,
well-conditioned physical derivative.

`conformance/reference.rs` explicitly selects this file for
`x86_64-unknown-linux-gnu`. Capture, drift checks, numerical verification and
the BJT production test share that selection. The same 291 columns are
checked at the original `1e-6` relative plus `1e-9` absolute bound. A missing
variant is an error; there is no existence-based fallback, widened floor or
column exclusion. Tests pin both captures' values and ensure a missing or
corrupted selected golden fails verification.

Reproduce without changing files:

```sh
NGSPICE_BIN=/absolute/path/to/linux/ngspice cargo xtask golden check --netlist sens_bjt
cargo xtask golden verify --netlist sens_bjt
```

The platform-specific path is a narrowly measured exception, not a promise
that all libm versions or untested targets have bit-identical arithmetic.
