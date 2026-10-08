//! Opt-in #45 cross-check against the local C binary.
//!
//! `ngspice` writes binary rawfiles unless `set filetype=ascii` is given, so the
//! same deck run twice — once with each filetype — produces a binary file and an
//! ASCII file that hold the same plot. This test asks the C binary for both, then
//! reads the binary one with the Rust reader and requires the values, names,
//! units, flags and plot count to match the ASCII oracle that
//! `crates/spice-analysis/tests/golden_rawfiles.rs` already pins.
//!
//! It is `#[ignore]`d because it needs a built C `ngspice` and writes temporary
//! decks out of process. Run it with an absolute `NGSPICE_BIN`:
//!
//! ```text
//! NGSPICE_BIN=/path/to/ngspice/build/src/ngspice \
//!   cargo test -p spice-analysis --test c_binary_rawfile_reference -- --ignored
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use spice_analysis::{BinaryByteOrder, PlotFlags, RawFile, RawFormat};
use spice_core::Complex;

/// The deck both C files come from: a resistive divider with a capacitor, so the
/// `.op` plot is `real` and the `.ac` plot is `complex`.
///
/// The AC sweep is `lin` because a `dec` sweep makes `raw_write()` append a
/// ` grid=3` option to the `frequency` line; the port's shared header parser
/// keeps a variable's name and unit and drops those options, exactly as the
/// ASCII reader already does, so a `dec` file would not be byte-identical. That
/// drop is covered by `binary_rawfiles::variable_options_are_dropped`.
const DECK: &str = "\
rawfile oracle
v1 in 0 dc 2 ac 1
r1 in out 1k
r2 out 0 1k
c1 out 0 1u
.control
set filetype=binary
op
write op_bin.raw
ac lin 4 100 3.2k
write ac_bin.raw
set filetype=ascii
op
write op_ascii.raw
ac lin 4 100 3.2k
write ac_ascii.raw
quit
.endc
.end
";

struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs temporary C decks out of process"]
fn c_binary_rawfiles_match_the_c_ascii_oracle() {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(Path::new(&binary).is_absolute());

    let directory =
        std::env::temp_dir().join(format!("spice-rawfile-oracle-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let _cleanup = Cleanup(directory.clone());
    fs::write(directory.join("oracle.cir"), DECK).unwrap();

    let output = Command::new(&binary)
        .args(["-b", "oracle.cir"])
        .current_dir(&directory)
        .output()
        .expect("the C ngspice runs");
    assert!(
        output.status.success(),
        "ngspice failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    for (binary_file, ascii_file) in [
        ("op_bin.raw", "op_ascii.raw"),
        ("ac_bin.raw", "ac_ascii.raw"),
    ] {
        let bytes = fs::read(directory.join(binary_file)).unwrap();
        assert_eq!(
            RawFormat::detect(&bytes).unwrap(),
            RawFormat::Binary,
            "{binary_file}: the C file is binary"
        );
        let got = RawFile::load(directory.join(binary_file)).unwrap();
        let want = RawFile::parse(
            &fs::read_to_string(directory.join(ascii_file))
                .unwrap_or_else(|error| panic!("{ascii_file}: {error}")),
        )
        .unwrap();

        assert_eq!(got.len(), want.len(), "{binary_file}: plot count");
        let got_header = &got.plots[0];
        let want_header = &want.plots[0];
        assert_eq!(got_header.title, want_header.title, "{binary_file}: title");
        assert_eq!(got_header.date, want_header.date, "{binary_file}: date");
        assert_eq!(
            got_header.command, want_header.command,
            "{binary_file}: command"
        );

        let got_plot = &got_header.plot;
        let want_plot = &want_header.plot;
        assert_eq!(got_plot.plotname, want_plot.plotname, "{binary_file}");
        assert_eq!(got_plot.flags, want_plot.flags, "{binary_file}: flags");
        assert_eq!(
            got_plot.variable_count(),
            want_plot.variable_count(),
            "{binary_file}: variable count"
        );
        assert_eq!(
            got_plot.point_count(),
            want_plot.point_count(),
            "{binary_file}: point count"
        );
        for (got_variable, want_variable) in got_plot.variables.iter().zip(&want_plot.variables) {
            assert_eq!(got_variable.name, want_variable.name, "{binary_file}: name");
            assert_eq!(got_variable.unit, want_variable.unit, "{binary_file}: unit");
        }

        // The payload is exact; the ASCII oracle prints `%.15e`, so only it can
        // be off by a ULP or two.
        for row in 0..want_plot.point_count() {
            for variable in &want_plot.variables {
                let read = got_plot.value(&variable.name, row).unwrap();
                let printed = want_plot.value(&variable.name, row).unwrap();
                let tolerance = 1e-14 * printed.magnitude() + 1e-300;
                assert!(
                    (read - printed).magnitude() <= tolerance,
                    "{binary_file}, {}, row {row}: binary={read}, ascii={printed}",
                    variable.name
                );
            }
        }

        // Our re-rendering of the bytes C wrote is byte-identical: same header,
        // same payload, same padding.
        assert_eq!(
            got.to_binary().unwrap(),
            bytes,
            "{binary_file}: byte-exact re-encoding"
        );
        // Both byte orders are available, and the big-endian payload is the
        // little-endian one reversed per 8-byte group.
        let big_endian = got.to_binary_with(BinaryByteOrder::BigEndian).unwrap();
        assert_eq!(big_endian.len(), bytes.len(), "{binary_file}");
        let little_endian = RawFile::parse_bytes(&big_endian).unwrap();
        assert_eq!(
            little_endian.to_binary().unwrap(),
            big_endian,
            "{binary_file}: byte order is explicit, not guessed"
        );
    }

    // The operating point is not only self-consistent: it is the divider's.
    let op = RawFile::load(directory.join("op_bin.raw")).unwrap();
    let plot = &op.plots[0].plot;
    assert_eq!(plot.flags, PlotFlags::Real);
    assert_eq!(plot.point_count(), 1);
    assert_eq!(plot.value("v(in)", 0), Some(Complex::real(2.0)));
    assert_eq!(plot.value("v(out)", 0), Some(Complex::real(1.0)));
    assert_eq!(plot.value("i(v1)", 0), Some(Complex::real(-1e-3)));

    // The AC plot is complex and holds the swept frequencies.
    let ac = RawFile::load(directory.join("ac_bin.raw")).unwrap();
    let plot = &ac.plots[0].plot;
    assert_eq!(plot.flags, PlotFlags::Complex);
    assert!(plot.variables.iter().all(|variable| !variable.is_real));
    assert_eq!(plot.point_count(), 4);
    let frequencies = plot.column("frequency").unwrap();
    assert_eq!(frequencies[0], Complex::new(1e2, 0.0));
    assert!(frequencies.windows(2).all(|pair| pair[0].re < pair[1].re));
    // The source is 1 V AC, and the RC network attenuates it.
    let input = plot.value("v(in)", 0).unwrap();
    assert!(
        (input - Complex::new(1.0, 0.0)).magnitude() < 1e-12,
        "{input}"
    );
    assert!(plot.value("v(out)", 0).unwrap().magnitude() < 1.0);
    assert!(
        plot.column("v(out)")
            .unwrap()
            .iter()
            .all(|v| v.magnitude() < 1.0)
    );
}
