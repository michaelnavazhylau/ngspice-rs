# Rawfiles: ASCII and binary I/O

Implemented for issue #45 in the Rust-only `work/m5-rawfiles` worktree, based on
`origin/main` `db0d3db`. The C reference is `src/frontend/rawfile.c` in a local
read-only checkout of upstream; C is never linked or called by the port, and no
payload is reinterpreted with `unsafe`.

Route map:

| Item | Where |
| --- | --- |
| ASCII parse/write (frozen behaviour) | `crates/spice-analysis/src/rawfile/mod.rs` |
| Binary codec, format detection, byte order | `crates/spice-analysis/src/rawfile/binary.rs` |
| Hand-built byte-buffer tests | `crates/spice-analysis/tests/binary_rawfiles.rs` |
| Opt-in live-C cross-check (`#[ignore]`) | `crates/spice-analysis/tests/c_binary_rawfile_reference.rs` |
| ASCII conformance against committed goldens | `crates/spice-analysis/tests/golden_rawfiles.rs`, `cargo xtask golden verify` |

## What ngspice writes

`raw_write()` (`rawfile.c:42-295`) writes one header for both encodings and then
branches. `set filetype=binary` selects the binary payload (`postcoms.c:592-600`
maps the option; `raw_write(file, &newplot, appendwrite, !ascii)` at
`postcoms.c:719`). `set filetype=ascii` is what `cargo xtask golden capture`
uses so that committed goldens stay diffable.

```
Title: <deck title>\n
Date: <ngspice's date string>\n
Command: <simulator>-<version>, Build <build date>\n
Plotname: <Operating Point | Transient Analysis | AC Analysis | ...>\n
Flags: real\n | Flags: complex\n        (plus " unpadded" when raw_padding is off)
No. Variables: <nvars>\n
No. Points: <length>\n                   (the longest vector in the plot)
[Dimensions: <dims>\n]                   (only when numdims > 1)
[Command: <pl_commands>]\n               (can repeat)
[Option: <name> = <value>]\n             (pl_env, can repeat)
Variables:\n
\t<i>\t<name>\t<type>[ min=][ max=][ color=][ grid=][ plot=][ dims=]\n   (per variable)
Values:\n | Binary:\n
<values>
```

The ASCII payload is text (`rawfile.c:260-293`); the binary payload is raw
`double`s (`rawfile.c:223-259`).

### ASCII values (unchanged)

One value per line, `\t%.15e` for a real plot and `\t%.15e,%.15e` for a complex
one. The first value of a point is preceded by ` i` (a space, the point index and
no tab), and every point is followed by a blank line. A vector that ngspice flags
real inside a complex plot is written as `re,0.0`, which is how the port recovers
`Variable::is_real`.

## Binary layout

```
<header as above, byte for byte identical to the ASCII header>
Binary:\n
<payload>
```

The payload is point-major and unaligned; it starts at the byte after the
`Binary:` newline. There is no length field, no magic number and no alignment:

| Plot | Bytes per value | Contents |
| --- | --- | --- |
| `Flags: real` | 8 | one `double`: the value's real part |
| `Flags: complex` | 16 | two `double`s: real part, then imaginary part |

Byte order is the writing machine's: `raw_write()` stores each value with a plain
`fwrite(&dd, sizeof(double), 1, fp)` (`rawfile.c:232` for a real plot, `:234-238`
for a real vector inside a complex plot, `:241-244` for a complex vector), so the
file records nothing about its order. The port reads and writes
little-endian (x86-64 and aarch64, the platforms it builds on) and exposes
`BinaryByteOrder::BigEndian` / `RawFileReader::with_byte_order` for a file from a
big-endian machine. Endianness is never guessed: a big-endian file read with the
default produces other values, one 8-byte group at a time, not an error.

The padding rule is `raw_write()`'s: when a vector is shorter than
`No. Points`, `raw_padding` (ngspice's default, `rawfile.c:57`) writes zeros for
the missing values (`rawfile.c:247-256`); `set nopadding` writes nothing and adds
` unpadded` to the `Flags:` line (`rawfile.c:119-120`). The port's `Plot` is
rectangular — every vector runs to `No. Points` — so it writes padded payloads
only, and a binary payload's length is exactly

```
No. Points x No. Variables x (8 or 16)
```

`raw_read()` reads exactly that many values (`rawfile.c:662-694`), which is why
an `unpadded` binary file, whose payload is shorter and whose per-vector lengths
are not recorded anywhere in the header, is rejected instead of guessed.

Multiple plots are concatenated, exactly as they are in ASCII: a payload ends and
the next plot's header begins at the next byte. `raw_read()` re-enters its line
loop, so a file of many binary plots is what C expects too.

That is also the layout of a multi-analysis batch run (`ngspice -b -r`, #96):
one plot per analysis, in `CKTdoJob()` order. The opt-in
`crates/spice-cli/tests/c_batch_reference.rs` reads such a C binary file with
`RawFile::parse_bytes` and compares it plot by plot with `spice-rs simulate`;
`crates/spice-analysis/tests/golden_rawfiles.rs`
(`multi_plot_goldens_round_trip_through_both_encodings`) round-trips the
committed four-plot golden `multi_analysis_rc.raw` through the ASCII and binary
writers. C's incremental batch writer (`OUTpData()` → `fileAddComplexValue()`
in `src/frontend/outitf.c`) writes the AC `frequency` reference as a complex
value whose imaginary half the AC driver never sets, so those bytes are not
data and the scale is compared by real part only.

## Reading and writing

```rust
// Detect the encoding, or select it.
let format = RawFormat::detect(&bytes)?;                  // Ascii | Binary
let rawfile = RawFile::parse_bytes(&bytes)?;              // detection + read
let rawfile = RawFile::load(path)?;                       // reads bytes, both forms
let rawfile = RawFileReader::new(&bytes)
    .with_format(RawFormat::Binary)                       // skip detection
    .with_byte_order(BinaryByteOrder::BigEndian)          // explicit order
    .read()?;

// Write either encoding.
rawfile.write_with_format(path, RawFormat::Binary)?;      // == to_binary()
rawfile.write_with_format(path, RawFormat::Ascii)?;       // == to_ascii()
let bytes = rawfile.to_binary()?;                         // little-endian
let bytes = rawfile.to_binary_with(BinaryByteOrder::BigEndian)?;
```

`RawFile::parse` still sees text only and still refuses a binary file; binary
bytes never pass through `str` beyond the (text) header, and the header is the
same text the ASCII form carries. Detection is a byte scan for the first
`Values:` or `Binary:` line, so a payload is never inspected as text.

The header renderer is shared (`render_header`), so a binary file's header is
byte-identical to the ASCII file's header for the same plot. `to_binary()` of a
rawfile read from a C binary file reproduces that file byte for byte only when
that file was already in this canonical form: per-variable options are dropped
(see below), and C's batch writer pads the `No. Points:` line and writes
`Title:` from the run's name (`outitf.c:1014-1048`), which the reader trims, so
a batch-written file parses correctly but re-renders canonically rather than
byte for byte.

## Validation before allocation

For a binary plot the codec computes the payload length with checked
multiplication and refuses the plot before allocating anything when

- `No. Points x No. Variables x width` overflows a `usize` ("overflows a usize");
- the result exceeds the bytes remaining in the file ("truncated binary rawfile:
  … needs N byte(s) of data, but only M byte(s) remain");
- the `Flags:` line carries `unpadded`;
- a declared `No. Variables:` is larger than the number of lines the file
  actually holds (the variable list grows as lines are read, so a bogus count
  cannot request a huge allocation).

Only after that does the reader allocate `No. Points` rows. The ASCII reader's
behaviour is unchanged, including its historical habit of preallocating from a
declared count; the port does not yet validate ASCII counts the same way.

## Rejected variants

Everything here fails with `SpiceError::Unsupported`, naming what was found:

| Variant | Why |
| --- | --- |
| `Flags: … unpadded` with `Binary:` | The payload length is not derivable from the header; C only skips values it does not need. |
| `unpadded` with `Values:` | Not rejected: the ASCII reader has always ignored the flag (`rawfile.c:411-414`), and its behaviour is frozen. |
| A file mixing `Values:` and `Binary:` plots | Detection picks the first plot's encoding (`rawfile.c:591-593` decides per plot); the other one is refused ("mixed rawfile encodings"). |
| `Dimensions: …` header or `dims=` on a variable line | The port's `Plot` is flat; multi-dimensional shape data has nowhere to go. `Dimensions:` is rejected as an unknown header key. |
| `Offset:` header | `raw_read()` also reports "Offset: is not supported" (`rawfile.c:374`). |
| Header bytes that are not UTF-8 | The port's headers are `String`s; a non-UTF-8 title cannot be represented. |
| Trailing bytes that do not start another plot | A truncated or appended file must not be silently ignored. |
| Unknown flag words (`Flags: real spectra`) | Not rejected: `raw_read()` prints "Warning: unknown flag" and continues (`rawfile.c:416`), so the port reads the data and says nothing. |
| Per-variable options (`min=`, `max=`, `color=`, `scale=`, `grid=`, `plot=`, `dims=`) | Accepted and dropped by the shared header parser (`rawfile.c:556-579`), name and unit kept. This is already the ASCII reader's behaviour for the committed goldens. For a `dec` AC sweep C writes `frequency grid=3`, so that file's variable line is not reproduced by `to_ascii()`/`to_binary()`; the payload still is. A padded payload's length does not depend on the options, so values are unaffected. |

Upstream in this checkout has no `fastaccess` `filetype` and no code path by that
name: `filetype` accepts `ascii` and `binary` only, and an unknown value is a
warning rather than an error. The two commands disagree about what it leaves
behind: `write` keeps its current default (`postcoms.c:592-599`, where `ascii`
defaults to `0`/binary in `conf.c:32`), while `run` warns and forces ASCII
(`runcoms.c:238-241`). There is therefore no `fastaccess` variant to accept or
reject; the port's rule is that an unknown header key, an unknown section line or
an `unpadded` payload is an error, never a guess.

## Known lossy point: `Variable::is_real`

`raw_write()` writes `(re, 0.0)` for a vector it has flagged real
(`isreal(v)`, `rawfile.c:234-238`, and `:271-272` in the ASCII branch) and
`(re, im)` otherwise, so the binary form
cannot distinguish a real vector from a complex one whose imaginary parts happen
to be zero. Reading a binary plot therefore sets `is_real` for every column of a
`real` plot and for no column of a `complex` plot, matching the complex vectors
`.ac` and `.noise` store. Values, names, units, `PlotFlags` and the plot count are
preserved exactly; the per-vector flag is not recoverable from binary.

The consequence for text output: re-rendering ASCII after a binary round trip
writes the long `re,0.000000000000000e+00` form where C wrote `re,0.0`, so
ASCII -> binary -> ASCII is not a fixed point for a `complex` plot with a real
column. The values, names, units, flags and plot count are identical;
`binary_rawfiles::a_binary_round_trip_loses_the_real_spelling_of_a_complex_column`
pins the difference.

## Tests

Ordinary tests build the exact byte buffers in Rust and need no C toolchain and
no committed binary blob:

```
cargo test -p spice-analysis --test binary_rawfiles
```

They cover a hand-built real plot, a hand-built complex plot, multiple plots,
byte-for-byte round trips, truncation, overflowing and oversized declared counts,
explicit byte order (and the fact that it is not detected), non-finite and
negative-zero values, `unpadded`, unknown header keys, mixed encodings, trailing
bytes, detection on a non-UTF-8 payload, and that `RawFile::parse` still refuses
binary text.

The opt-in cross-check runs the local C `ngspice`, which writes a binary and an
ASCII rawfile for the same deck, and requires the binary reader to agree with the
ASCII oracle on values, names, units, flags and plot count — and to re-encode
C's bytes exactly:

```
NGSPICE_BIN=/Users/michaelnavazhylau/Code/electronics-work/spice-port/ngspice_test/build/src/ngspice \
  cargo test -p spice-analysis --test c_binary_rawfile_reference -- --ignored
```

## Commands

```
cargo fmt --all
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo xtask golden verify
```

`cargo xtask golden verify` stays at `0 failure(s)` and the committed ASCII
goldens parse to identical values: the ASCII parse and write paths are unchanged.
