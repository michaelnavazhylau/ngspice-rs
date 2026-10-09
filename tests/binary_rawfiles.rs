//! The binary rawfile codec, driven by hand-constructed byte buffers.
//!
//! Every buffer below is built in Rust from the layout documented in
//! `docs/port/RAWFILES.md`, derived from `raw_write()`/`raw_read()` in
//! `src/frontend/rawfile.c`. The tests therefore need no C toolchain and no
//! committed binary blob;
//! `c_binary_rawfile_reference` is the opt-in cross-check against the local C
//! binary.
//!
//! The layout under test: the ASCII header, then `Binary:\n`, then point-major
//! little-endian `double`s — one per value in a `real` plot, `re` and `im` for
//! each value in a `complex` plot.

use std::fs;
use std::path::PathBuf;
use std::str;

use ngspice_rs::analysis::{BinaryByteOrder, PlotFlags, RawFile, RawFileReader, RawFormat};
use ngspice_rs::primitives::Complex;

/// A `real` operating-point header: three variables, two points.
const OP_HEADER: &str = "\
Title: rc divider
Date: Mon Oct  5 17:53:58 2026
Command: ngspice-47+, Build
Plotname: Operating Point
Flags: real
No. Variables: 3
No. Points: 2
Variables:
\t0\tv(in)\tvoltage
\t1\tv(out)\tvoltage
\t2\ti(v1)\tcurrent
";

/// The values of [`OP_HEADER`], point-major: two points of three values.
const OP_VALUES: [f64; 6] = [2.0, 1.0, -1e-3, 3.0, 1.5, -5e-4];

/// A `complex` AC header: two variables, two points.
const AC_HEADER: &str = "\
Title: rc lowpass
Date: Mon Oct  5 18:00:00 2026
Command: ngspice-47+, Build
Plotname: AC Analysis
Flags: complex
No. Variables: 2
No. Points: 2
Variables:
\t0\tfrequency\tfrequency
\t1\tv(out)\tvoltage
";

/// The values of [`AC_HEADER`], point-major: two points of `re, im` pairs.
const AC_VALUES: [f64; 8] = [1e2, 0.0, 0.99, -0.01, 1e3, 0.0, 0.9, -0.1];

/// A small ASCII rawfile, for the tests that contrast the two encodings.
const ASCII_RAWFILE: &str = "\
Title: rc divider
Date: Mon Oct  5 17:53:58 2026
Command: ngspice-47+, Build
Plotname: Operating Point
Flags: real
No. Variables: 1
No. Points: 1
Variables:
\t0\tv(out)\tvoltage
Values:
 0\t2.500000000000000e+00
";

/// Builds a binary rawfile: `header`, `Binary:`, then `values` point-major.
fn binary_rawfile(header: &str, values: &[f64]) -> Vec<u8> {
    let mut bytes = header.as_bytes().to_vec();
    bytes.extend_from_slice(b"Binary:\n");
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// The same, with the payload in big-endian order.
fn big_endian_rawfile(header: &str, values: &[f64]) -> Vec<u8> {
    let mut bytes = header.as_bytes().to_vec();
    bytes.extend_from_slice(b"Binary:\n");
    for value in values {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes
}

struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn reads_a_hand_constructed_real_plot() {
    let bytes = binary_rawfile(OP_HEADER, &OP_VALUES);
    assert_eq!(RawFormat::detect(&bytes).unwrap(), RawFormat::Binary);

    let rawfile = RawFile::parse_bytes(&bytes).expect("parses");
    assert_eq!(rawfile.len(), 1);
    let raw_plot = &rawfile.plots[0];
    assert_eq!(raw_plot.title, "rc divider");
    assert_eq!(raw_plot.date, "Mon Oct  5 17:53:58 2026");
    assert_eq!(raw_plot.command, "ngspice-47+, Build");

    let plot = &raw_plot.plot;
    assert_eq!(plot.plotname, "Operating Point");
    assert_eq!(plot.name, "Operating Point");
    assert_eq!(plot.flags, PlotFlags::Real);
    assert_eq!(plot.variable_count(), 3);
    assert_eq!(plot.point_count(), 2);
    let names: Vec<&str> = plot.variables.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["v(in)", "v(out)", "i(v1)"]);
    let units: Vec<&str> = plot.variables.iter().map(|v| v.unit.as_str()).collect();
    assert_eq!(units, ["voltage", "voltage", "current"]);
    assert!(plot.variables.iter().all(|variable| variable.is_real));

    assert_eq!(plot.value("v(in)", 0), Some(Complex::real(2.0)));
    assert_eq!(plot.value("v(out)", 0), Some(Complex::real(1.0)));
    assert_eq!(plot.value("i(v1)", 1), Some(Complex::real(-5e-4)));
    assert_eq!(plot.value("v(in)", 1), Some(Complex::real(3.0)));
    assert_eq!(plot.value("v(out)", 2), None);
}

#[test]
fn reads_a_hand_constructed_complex_plot() {
    let bytes = binary_rawfile(AC_HEADER, &AC_VALUES);
    assert_eq!(RawFormat::detect(&bytes).unwrap(), RawFormat::Binary);

    let rawfile = RawFile::parse_bytes(&bytes).expect("parses");
    let plot = &rawfile.plots[0].plot;
    assert_eq!(plot.flags, PlotFlags::Complex);
    assert_eq!(plot.point_count(), 2);
    // The binary form does not record `isreal(v)`, so no column claims to be
    // real; see `docs/port/RAWFILES.md`.
    assert!(plot.variables.iter().all(|variable| !variable.is_real));

    assert_eq!(plot.value("frequency", 0), Some(Complex::new(1e2, 0.0)));
    assert_eq!(plot.value("v(out)", 0), Some(Complex::new(0.99, -0.01)));
    assert_eq!(plot.value("v(out)", 1), Some(Complex::new(0.9, -0.1)));
}

#[test]
fn multiple_binary_plots_keep_their_order_and_count() {
    let mut bytes = binary_rawfile(OP_HEADER, &OP_VALUES);
    bytes.extend_from_slice(&binary_rawfile(AC_HEADER, &AC_VALUES));

    let rawfile = RawFile::parse_bytes(&bytes).expect("parses");
    assert_eq!(rawfile.len(), 2);
    assert_eq!(rawfile.plots[0].plot.plotname, "Operating Point");
    assert_eq!(rawfile.plots[1].plot.plotname, "AC Analysis");
    assert_eq!(rawfile.plots[0].plot.flags, PlotFlags::Real);
    assert_eq!(rawfile.plots[1].plot.flags, PlotFlags::Complex);
    assert_eq!(
        rawfile.plots[1].plot.value("v(out)", 1),
        Some(Complex::new(0.9, -0.1))
    );
    assert!(rawfile.single_plot().is_err());
    // The second plot starts exactly where the first payload ends.
    assert_eq!(rawfile.to_binary().unwrap(), bytes);
}

#[test]
fn both_encodings_round_trip_byte_for_byte() {
    let mut two_plots = binary_rawfile(OP_HEADER, &OP_VALUES);
    two_plots.extend_from_slice(&binary_rawfile(AC_HEADER, &AC_VALUES));
    for bytes in [
        binary_rawfile(OP_HEADER, &OP_VALUES),
        binary_rawfile(AC_HEADER, &AC_VALUES),
        two_plots,
    ] {
        let parsed = RawFile::parse_bytes(&bytes).expect("parses");
        let rendered = parsed.to_binary().expect("renders");
        assert_eq!(rendered, bytes, "the layout is reproduced byte for byte");
        assert_eq!(RawFile::parse_bytes(&rendered).unwrap(), parsed);
    }
}

#[test]
fn truncated_payloads_name_the_missing_bytes() {
    let bytes = binary_rawfile(OP_HEADER, &OP_VALUES);
    let expected = OP_VALUES.len() * 8;
    for missing in [1, 8, 47] {
        let truncated = &bytes[..bytes.len() - missing];
        let error = RawFile::parse_bytes(truncated).expect_err("truncated");
        let message = error.to_string();
        assert!(message.contains("truncated binary rawfile"), "{message}");
        assert!(
            message.contains(&format!("needs {expected} byte(s) of data")),
            "{message}"
        );
        assert!(
            message.contains(&format!("only {} byte(s) remain", expected - missing)),
            "{message}"
        );
    }
    // A payload of exactly the declared size is the boundary.
    assert!(RawFile::parse_bytes(&bytes).is_ok());
    // The header alone, with no `Binary:` line at all, is truncated too.
    assert!(RawFile::parse_bytes(OP_HEADER.as_bytes()).is_err());
}

#[test]
fn declared_counts_are_validated_before_allocation() {
    // Counts whose payload size cannot even be represented are rejected. The
    // literals are width-independent so a 32-bit target takes the same branch.
    let header = OP_HEADER.replace("No. Points: 2", &format!("No. Points: {}", usize::MAX));
    let error = RawFile::parse_bytes(&binary_rawfile(&header, &OP_VALUES)).expect_err("overflows");
    assert!(error.to_string().contains("overflows a usize"), "{error}");

    // A count that fits a usize but not the file is caught by the byte length,
    // not by an allocation of that many bytes.
    let huge = OP_VALUES.len() * 8 + 1000;
    let header = OP_HEADER.replace("No. Points: 2", &format!("No. Points: {huge}"));
    let error = RawFile::parse_bytes(&binary_rawfile(&header, &OP_VALUES)).expect_err("huge");
    let message = error.to_string();
    assert!(message.contains("truncated binary rawfile"), "{message}");
    assert!(
        message.contains(&format!("No. Points: {huge}")),
        "{message}"
    );

    // An absurd variable count is a claim the file has to back with lines.
    let header = OP_HEADER.replace(
        "No. Variables: 3",
        &format!("No. Variables: {}", usize::MAX),
    );
    let error =
        RawFile::parse_bytes(&binary_rawfile(&header, &OP_VALUES)).expect_err("no such lines");
    assert!(error.to_string().contains("variable line"), "{error}");
}

#[test]
fn byte_order_is_explicit_and_never_detected() {
    let bytes = big_endian_rawfile(OP_HEADER, &OP_VALUES);
    let big_endian = RawFileReader::new(&bytes)
        .with_byte_order(BinaryByteOrder::BigEndian)
        .read()
        .expect("parses with an explicit byte order");
    assert_eq!(
        big_endian.plots[0].plot.value("v(in)", 0),
        Some(Complex::real(2.0))
    );

    // The file records nothing about its byte order, so the default
    // little-endian decode cannot recover a big-endian payload. It is not
    // guessed at either: the caller has to ask.
    let little_endian = RawFile::parse_bytes(&bytes).expect("still reads bytes");
    assert_ne!(
        little_endian.plots[0].plot.value("v(in)", 0),
        Some(Complex::real(2.0))
    );

    // Writing selects the order the same way: the values read with an explicit
    // big-endian order render as little-endian bytes, and pinning big-endian
    // again reproduces the file.
    assert_eq!(
        big_endian.to_binary().unwrap(),
        binary_rawfile(OP_HEADER, &OP_VALUES)
    );
    assert_eq!(
        big_endian
            .to_binary_with(BinaryByteOrder::BigEndian)
            .unwrap(),
        bytes
    );
    // A wrong-order read is not garbage: it decodes the payload to other values,
    // one 8-byte group at a time, so re-encoding them reproduces the file.
    assert_eq!(little_endian.to_binary().unwrap(), bytes);
}

#[test]
fn non_finite_samples_survive_the_round_trip() {
    let header = OP_HEADER
        .replace("No. Variables: 3", "No. Variables: 1")
        .replace("No. Points: 2", "No. Points: 4")
        .replace("\t1\tv(out)\tvoltage\n\t2\ti(v1)\tcurrent\n", "");
    let values = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0];
    let bytes = binary_rawfile(&header, &values);

    let rawfile = RawFile::parse_bytes(&bytes).expect("parses");
    let column = rawfile.plots[0].plot.column("v(in)").expect("column");
    assert!(column[0].re.is_nan());
    assert_eq!(column[1].re, f64::INFINITY);
    assert_eq!(column[2].re, f64::NEG_INFINITY);
    assert_eq!(column[3].re.to_bits(), (-0.0_f64).to_bits());
    assert!(!rawfile.plots[0].plot.is_finite());
    // The NaN payload is copied bit for bit.
    assert_eq!(rawfile.to_binary().unwrap(), bytes);

    // A complex plot stores the same `double`s, `im` included.
    let values = [
        1e3,
        0.0,
        f64::NAN,
        f64::INFINITY,
        2e3,
        0.0,
        0.5,
        f64::NEG_INFINITY,
    ];
    let bytes = binary_rawfile(AC_HEADER, &values);
    let rawfile = RawFile::parse_bytes(&bytes).expect("parses");
    let plot = &rawfile.plots[0].plot;
    let value = plot.value("v(out)", 0).unwrap();
    assert!(value.re.is_nan() && value.im == f64::INFINITY);
    assert_eq!(
        plot.value("v(out)", 1),
        Some(Complex::new(0.5, f64::NEG_INFINITY))
    );
    assert_eq!(rawfile.to_binary().unwrap(), bytes);
}

#[test]
fn unpadded_binary_payloads_are_rejected_and_ascii_still_ignores_the_flag() {
    let header = OP_HEADER.replace("Flags: real", "Flags: real unpadded");
    let error = RawFile::parse_bytes(&binary_rawfile(&header, &OP_VALUES)).expect_err("unpadded");
    assert!(error.to_string().contains("unpadded"), "{error}");

    // The ASCII parser has always ignored the flag, and still does.
    let ascii = ASCII_RAWFILE.replace("Flags: real", "Flags: real unpadded");
    let rawfile = RawFile::parse_bytes(ascii.as_bytes()).expect("ASCII ignores the flag");
    assert_eq!(
        rawfile.plots[0].plot.value("v(out)", 0),
        Some(Complex::real(2.5))
    );
}

#[test]
fn unsupported_header_variants_are_rejected_explicitly() {
    // `Dimensions:` shape data (multi-dimensional plots) has no home in the
    // port's flat `Plot`, so it fails instead of being dropped.
    let header = OP_HEADER.replace("No. Points: 2", "No. Points: 2\nDimensions: 3 2");
    let error = RawFile::parse_bytes(&binary_rawfile(&header, &OP_VALUES)).expect_err("Dimensions");
    assert!(error.to_string().contains("Dimensions"), "{error}");

    // `raw_read()` warns that `Offset:` is unsupported; the port rejects it.
    let header = format!("Offset: 12\n{OP_HEADER}");
    let error = RawFile::parse_bytes(&binary_rawfile(&header, &OP_VALUES)).expect_err("Offset");
    assert!(error.to_string().contains("Offset"), "{error}");

    let header = OP_HEADER.replace("No. Points: 2", "No. Points: 2\nBogus: 1");
    let error = RawFile::parse_bytes(&binary_rawfile(&header, &OP_VALUES)).expect_err("Bogus");
    assert!(error.to_string().contains("header key 'Bogus'"), "{error}");
}

#[test]
fn variable_options_are_dropped_like_the_ascii_reader_drops_them() {
    // A `dec` AC sweep makes `raw_write()` write `frequency grid=3`: a variable
    // line carries the name, the unit and then per-variable options (`min=`,
    // `max=`, `color=`, `scale=`, `grid=`, `plot=`, `dims=`). The port keeps the
    // name and the unit and drops the options, which is what the ASCII reader
    // already does; a padded payload's length does not depend on them.
    let header = AC_HEADER.replace(
        "\t0\tfrequency\tfrequency\n",
        "\t0\tfrequency\tfrequency grid=3\n",
    );
    let rawfile = RawFile::parse_bytes(&binary_rawfile(&header, &AC_VALUES)).expect("parses");
    let plot = &rawfile.plots[0].plot;
    assert_eq!(plot.variables[0].name, "frequency");
    assert_eq!(plot.variables[0].unit, "frequency");
    assert_eq!(plot.value("frequency", 0), Some(Complex::new(1e2, 0.0)));

    // Re-rendering drops exactly the option and changes nothing else.
    assert_eq!(
        rawfile.to_binary().unwrap(),
        binary_rawfile(AC_HEADER, &AC_VALUES)
    );
}

#[test]
fn unknown_flag_words_are_ignored_as_the_c_reader_ignores_them() {
    // `raw_read()` prints "Warning: unknown flag %s" and carries on, so the port
    // reads the data and says nothing; only `unpadded` changes what is accepted.
    let header = OP_HEADER.replace("Flags: real", "Flags: real spectra");
    let rawfile = RawFile::parse_bytes(&binary_rawfile(&header, &OP_VALUES)).expect("parses");
    let plot = &rawfile.plots[0].plot;
    assert_eq!(plot.flags, PlotFlags::Real);
    assert_eq!(plot.value("v(in)", 1), Some(Complex::real(3.0)));
}

#[test]
fn mixed_ascii_and_binary_plots_are_rejected() {
    // Binary plot first: the first section decides the encoding, and the ASCII
    // plot that follows cannot be read as binary.
    let mut bytes = binary_rawfile(OP_HEADER, &OP_VALUES);
    bytes.extend_from_slice(ASCII_RAWFILE.as_bytes());
    assert_eq!(RawFormat::detect(&bytes).unwrap(), RawFormat::Binary);
    let error = RawFile::parse_bytes(&bytes).expect_err("mixed");
    assert!(
        error.to_string().contains("mixed rawfile encodings"),
        "{error}"
    );

    // ASCII plot first: detection reports ASCII, and asking for binary anyway is
    // refused rather than guessed.
    let mut bytes = ASCII_RAWFILE.as_bytes().to_vec();
    bytes.extend_from_slice(&binary_rawfile(AC_HEADER, &AC_VALUES));
    assert_eq!(RawFormat::detect(&bytes).unwrap(), RawFormat::Ascii);
    let error = RawFileReader::new(&bytes)
        .with_format(RawFormat::Binary)
        .read()
        .expect_err("mixed");
    assert!(
        error.to_string().contains("mixed rawfile encodings"),
        "{error}"
    );
}

#[test]
fn bytes_after_the_last_payload_are_rejected() {
    let mut bytes = binary_rawfile(OP_HEADER, &OP_VALUES);
    bytes.extend_from_slice(b"junk\n");
    let error = RawFile::parse_bytes(&bytes).expect_err("trailing bytes");
    assert!(
        error
            .to_string()
            .contains("no 'Values:' or 'Binary:' section"),
        "{error}"
    );

    // A second plot's header that stops before any section line is truncated.
    let mut bytes = binary_rawfile(OP_HEADER, &OP_VALUES);
    bytes.extend_from_slice(AC_HEADER.as_bytes());
    let error = RawFile::parse_bytes(&bytes).expect_err("no section line");
    assert!(
        error
            .to_string()
            .contains("no 'Values:' or 'Binary:' section"),
        "{error}"
    );

    // A complete second header whose declared payload is cut short is a
    // truncation with a byte count, not a missing section.
    let mut bytes = binary_rawfile(OP_HEADER, &OP_VALUES);
    bytes.extend_from_slice(AC_HEADER.as_bytes());
    bytes.extend_from_slice(b"Binary:\n");
    bytes.extend_from_slice(&[0; 3]);
    let error = RawFile::parse_bytes(&bytes).expect_err("short payload");
    assert!(
        error.to_string().contains("needs 64 byte(s) of data"),
        "{error}"
    );
}

#[test]
fn detection_reads_only_the_header_and_never_decodes_the_payload() {
    assert_eq!(
        RawFormat::detect(ASCII_RAWFILE.as_bytes()).unwrap(),
        RawFormat::Ascii
    );
    assert_eq!(
        RawFormat::detect(&binary_rawfile(OP_HEADER, &OP_VALUES)).unwrap(),
        RawFormat::Binary
    );
    assert!(RawFormat::detect(b"Title: x\n").is_err());
    assert!(RawFormat::detect(b"binary:\n").is_err());

    // A payload that is not valid UTF-8 — the `0xFF` NaN pattern — never goes
    // through text parsing.
    let mut bytes = OP_HEADER.as_bytes().to_vec();
    bytes.extend_from_slice(b"Binary:\n");
    bytes.extend_from_slice(&[0xFF; 48]);
    assert!(str::from_utf8(&bytes).is_err());
    assert_eq!(RawFormat::detect(&bytes).unwrap(), RawFormat::Binary);
    let rawfile = RawFile::parse_bytes(&bytes).expect("parses");
    assert!(
        rawfile.plots[0]
            .plot
            .points
            .iter()
            .flatten()
            .all(|value| value.re.is_nan())
    );
}

#[test]
fn the_byte_reader_keeps_the_ascii_contract() {
    assert_eq!(
        RawFile::parse_bytes(ASCII_RAWFILE.as_bytes()).unwrap(),
        RawFile::parse(ASCII_RAWFILE).unwrap()
    );

    // `RawFile::parse` sees text only and still refuses binary input.
    let error = RawFile::parse(&format!("{OP_HEADER}Binary:\n")).expect_err("binary text");
    assert!(error.to_string().contains("binary rawfile"), "{error}");

    // A file with no content holds no plots, as it always has.
    assert!(RawFile::parse_bytes(b"").unwrap().is_empty());
    assert!(RawFile::parse_bytes(b"\n\n").unwrap().is_empty());
}

#[test]
fn files_round_trip_through_both_encodings() {
    let directory = std::env::temp_dir().join(format!("spice-rawfile-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let _cleanup = Cleanup(directory.clone());

    let bytes = binary_rawfile(OP_HEADER, &OP_VALUES);
    let parsed = RawFile::parse_bytes(&bytes).unwrap();

    let binary_path = directory.join("op_bin.raw");
    parsed
        .write_with_format(&binary_path, RawFormat::Binary)
        .unwrap();
    assert_eq!(fs::read(&binary_path).unwrap(), bytes);
    assert_eq!(RawFile::load(&binary_path).unwrap(), parsed);

    let ascii_path = directory.join("op_ascii.raw");
    parsed
        .write_with_format(&ascii_path, RawFormat::Ascii)
        .unwrap();
    assert_eq!(fs::read(&ascii_path).unwrap(), parsed.to_ascii().as_bytes());
    assert_eq!(RawFile::load(&ascii_path).unwrap(), parsed);
}

/// A `complex` plot whose first column C wrote in the `isreal(v)` short form.
const REAL_SPELLING_RAWFILE: &str = "\
Title: noise
Date: Mon Oct  5 18:00:00 2026
Command: ngspice-47+, Build
Plotname: Noise Analysis
Flags: complex
No. Variables: 2
No. Points: 1
Variables:
\t0\tfrequency\tfrequency
\t1\tinoise_spectrum\tnoise-spectral-density
Values:
 0\t1.000000000000000e+03,0.0
\t1.000000000000000e-18,0.000000000000000e+00

";

#[test]
fn a_binary_round_trip_loses_the_real_spelling_of_a_complex_column() {
    // `isreal(v)` is written as `re,0.0` and cannot be recovered from the
    // payload, so re-rendering ASCII after a binary round trip writes the long
    // form. Values, names, units, flags and plot count survive; only the
    // spelling of a real column inside a `complex` plot changes.
    let parsed = RawFile::parse(REAL_SPELLING_RAWFILE).unwrap();
    let before = parsed.single_plot().unwrap();
    assert!(
        before.variables[0].is_real,
        "frequency was written `re,0.0`"
    );
    let round_tripped = RawFile::parse_bytes(&parsed.to_binary().unwrap()).unwrap();
    let after = round_tripped.single_plot().unwrap();
    assert!(!after.variables[0].is_real, "the flag is not in the bytes");
    assert_eq!(after.flags, before.flags);
    assert_eq!(after.variable_count(), before.variable_count());
    assert_eq!(after.point_count(), before.point_count());
    assert_eq!(
        after.column("frequency").unwrap(),
        before.column("frequency").unwrap()
    );
    // The documented consequence: the spelling changes, the values do not.
    assert!(
        parsed.to_ascii().contains("1.000000000000000e+03,0.0"),
        "the parsed text keeps the short form"
    );
    assert!(
        round_tripped
            .to_ascii()
            .contains("1.000000000000000e+03,0.000000000000000e+00"),
        "the re-rendered text uses the long form"
    );
}
