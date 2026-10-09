//! The binary rawfile codec: native `double` payloads behind a text header.
//!
//! Ported from the `if (binary)` branch of `raw_write()`
//! (`src/frontend/rawfile.c:74-91` for the file mode, `:223-259` for the
//! payload) and from the `is_ascii == FALSE` branch of `raw_read()`
//! (`:591-593` for the `Values:`/`Binary:` dispatch, `:667-694` for the reads).
//! See `docs/port/RAWFILES.md` for the same layout in table form and for the
//! variants this codec deliberately rejects.
//!
//! A binary rawfile is the ASCII header, the line `Binary:`, and then the
//! payload:
//!
//! | bytes | field |
//! | --- | --- |
//! | text | `Title:`, `Date:`, `Command:`, `Plotname:`, `Flags:`, `No. Variables:`, `No. Points:`, `Variables:` and one line per variable — byte for byte what the ASCII form writes |
//! | text | `Binary:` |
//! | payload | point-major values, no alignment or padding: for every point, every variable in header order |
//!
//! One value occupies 8 bytes in a `real` plot (the real part) and 16 bytes in a
//! `complex` plot (`re` then `im`). Every value is a C `double` stored with a
//! plain `fwrite(&dd, sizeof(double), 1, fp)`, so the payload is 8-byte
//! IEEE-754 binary64 in the writing machine's native byte order; the file
//! records nothing about that order, so [`RawFileReader::with_byte_order`] is the
//! only way to read a payload other than the little-endian default.
//!
//! `raw_read()` reads exactly `No. Points:` x `No. Variables:` values
//! (`rawfile.c:637-694`) and skips values it does not need only when the plot is
//! padded (the default). This codec therefore requires a padded payload: the
//! length of an unpadded one is not derivable from the header, and the port's
//! [`Plot`] is rectangular.

use std::str;

use crate::primitives::{Complex, Real, SpiceResult};

use super::{Cursor, Header, RawFile, RawPlot, Section, parse_header, render_header, unsupported};
use crate::analysis::results::{Plot, PlotFlags};

/// Bytes in one stored `double`.
const DOUBLE_BYTES: usize = 8;

/// Raised when the bytes hold no `Values:` or `Binary:` line at all.
const NO_SECTION: &str = "truncated rawfile: no 'Values:' or 'Binary:' section";

/// The encoding a rawfile uses for its values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RawFormat {
    /// Text values under a `Values:` line — `set filetype=ascii`, and what
    /// `cargo xtask golden capture` records.
    Ascii,
    /// Native `double`s under a `Binary:` line — `raw_write()`'s default.
    Binary,
}

impl RawFormat {
    /// Detects the encoding from the first data-section line the bytes hold.
    ///
    /// Only header lines are inspected, byte by byte: nothing that follows a
    /// section line is decoded as text.
    ///
    /// # Errors
    ///
    /// [`crate::primitives::SpiceError::Unsupported`] when there is no `Values:` or
    /// `Binary:` line, which is what a truncated, malformed or empty file looks
    /// like.
    pub fn detect(bytes: &[u8]) -> SpiceResult<Self> {
        let Some((_, section)) = find_section_line(bytes, 0) else {
            return Err(unsupported(NO_SECTION));
        };
        Ok(match section {
            Section::Values => Self::Ascii,
            Section::Binary => Self::Binary,
        })
    }

    /// True for [`RawFormat::Binary`].
    #[must_use]
    pub const fn is_binary(self) -> bool {
        matches!(self, Self::Binary)
    }
}

/// Byte order of the `double`s in a binary payload.
///
/// `fwrite()` stores the writing machine's representation, and the file does not
/// say which that was, so the reader never guesses: it decodes little-endian
/// unless [`RawFileReader::with_byte_order`] selects otherwise. Little-endian is
/// what x86-64 and aarch64 use, the platforms the port supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BinaryByteOrder {
    /// The order [`RawFile::to_binary`] writes.
    #[default]
    LittleEndian,
    /// For a rawfile written on a big-endian machine.
    BigEndian,
}

impl BinaryByteOrder {
    /// Decodes one stored `double`.
    fn decode(self, bytes: [u8; DOUBLE_BYTES]) -> Real {
        match self {
            Self::LittleEndian => Real::from_le_bytes(bytes),
            Self::BigEndian => Real::from_be_bytes(bytes),
        }
    }

    /// Encodes one `double` for storage.
    fn encode(self, value: Real) -> [u8; DOUBLE_BYTES] {
        match self {
            Self::LittleEndian => value.to_le_bytes(),
            Self::BigEndian => value.to_be_bytes(),
        }
    }
}

/// Reads a rawfile from in-memory bytes without ever decoding a payload as text.
///
/// [`RawFileReader::new`] detects the encoding of the file. Use
/// [`RawFileReader::with_format`] to select an encoding instead of detecting it
/// and [`RawFileReader::with_byte_order`] for a binary payload that is not
/// little-endian.
#[derive(Debug, Clone, Copy)]
pub struct RawFileReader<'a> {
    bytes: &'a [u8],
    format: Option<RawFormat>,
    byte_order: BinaryByteOrder,
}

impl<'a> RawFileReader<'a> {
    /// A reader that detects the encoding and reads `double`s little-endian.
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            format: None,
            byte_order: BinaryByteOrder::default(),
        }
    }

    /// Uses `format` instead of detecting it.
    #[must_use]
    pub fn with_format(mut self, format: RawFormat) -> Self {
        self.format = Some(format);
        self
    }

    /// Reads a binary payload in `byte_order`.
    #[must_use]
    pub fn with_byte_order(mut self, byte_order: BinaryByteOrder) -> Self {
        self.byte_order = byte_order;
        self
    }

    /// The encoding this reader will use: the selected one, or the detected one.
    ///
    /// # Errors
    ///
    /// As [`RawFormat::detect`].
    pub fn format(&self) -> SpiceResult<RawFormat> {
        match self.format {
            Some(format) => Ok(format),
            None => RawFormat::detect(self.bytes),
        }
    }

    /// Parses the rawfile.
    ///
    /// # Errors
    ///
    /// [`crate::primitives::SpiceError::Unsupported`] for everything the ASCII parser
    /// rejects, and for the binary variants this codec rejects: a payload that
    /// does not fit the declared `No. Variables:`/`No. Points:`, counts whose
    /// payload size overflows, an `unpadded` payload, a header that is not UTF-8
    /// text, a `Values:` section inside a binary file, or bytes after the last
    /// payload that do not start another plot.
    pub fn read(&self) -> SpiceResult<RawFile> {
        if is_blank(self.bytes) {
            // A file with no content holds no plots, exactly as `RawFile::parse`
            // reports for empty text.
            return Ok(RawFile { plots: Vec::new() });
        }
        match self.format()? {
            RawFormat::Ascii => {
                let text = str::from_utf8(self.bytes).map_err(|_| {
                    unsupported("rawfile is not UTF-8 text, so it is not the ASCII form")
                })?;
                RawFile::parse(text)
            }
            RawFormat::Binary => read_binary(self.bytes, self.byte_order),
        }
    }
}

/// Reads every plot in a binary `bytes`, decoding with `byte_order`.
fn read_binary(bytes: &[u8], byte_order: BinaryByteOrder) -> SpiceResult<RawFile> {
    let mut plots = Vec::new();
    let mut position = 0;
    while let Some((raw_plot, next)) = read_plot(bytes, position, byte_order)? {
        plots.push(raw_plot);
        position = next;
    }
    Ok(RawFile { plots })
}

/// Reads one binary plot at or after `from`, with the offset its payload ends at.
///
/// Returns `None` when only whitespace remains.
fn read_plot(
    bytes: &[u8],
    from: usize,
    byte_order: BinaryByteOrder,
) -> SpiceResult<Option<(RawPlot, usize)>> {
    // `raw_write()` emits no blank line in binary mode, but a file whose plots
    // were assembled from the ASCII writer can carry them between plots.
    let mut start = from;
    while start < bytes.len() && matches!(bytes[start], b'\n' | b'\r') {
        start += 1;
    }
    if is_blank(&bytes[start..]) {
        return Ok(None);
    }

    // The scanner and the header parser walk the same lines in the same order:
    // `find_section_line` finds the line that ends this header, `parse_header`
    // validates it and returns the rest of the header, and `header_end` is
    // therefore the first byte of the payload.
    let Some((header_end, section)) = find_section_line(bytes, start) else {
        return Err(unsupported(NO_SECTION));
    };
    let text = str::from_utf8(&bytes[start..header_end])
        .map_err(|_| unsupported("rawfile header is not UTF-8 text"))?;
    let Some((header, _)) = parse_header(&mut Cursor::new(text))? else {
        return Err(unsupported("truncated rawfile: no header"));
    };
    if section == Section::Values {
        return Err(unsupported(
            "mixed rawfile encodings: a 'Values:' section cannot appear in a binary rawfile",
        ));
    }
    if !header.padded {
        return Err(unsupported(
            "binary rawfile flagged 'unpadded': its payload length cannot be derived from the header",
        ));
    }

    let columns = header.variables.len();
    let width = value_width(header.flags);
    let payload_len = header
        .point_count
        .checked_mul(columns)
        .and_then(|cells| cells.checked_mul(width));
    let Some(payload_len) = payload_len else {
        return Err(unsupported(format!(
            "binary rawfile declares No. Variables: {columns} and No. Points: {}, whose payload size overflows a usize",
            header.point_count
        )));
    };
    let available = bytes.len() - header_end;
    if payload_len > available {
        return Err(unsupported(format!(
            "truncated binary rawfile: No. Variables: {columns} x No. Points: {} needs {payload_len} byte(s) of data, but only {available} byte(s) remain",
            header.point_count
        )));
    }

    // The payload was shown to fit in the input before either allocation below,
    // so neither can be driven by a count the file does not actually carry.
    let points = if columns == 0 {
        // A plot without variables has no payload; the ASCII parser reports no
        // points for it either.
        Vec::new()
    } else {
        decode_points(bytes, header_end, &header, byte_order)
    };

    let Header {
        title,
        date,
        command,
        plotname,
        flags,
        mut variables,
        ..
    } = header;
    if flags.is_complex() {
        // The binary form does not record `isreal(v)`: `raw_write()` stores
        // `(re, 0.0)` for a vector flagged real and `(re, im)` otherwise, which
        // no value can tell apart when `im` is zero. A real plot flags every
        // column real; a complex plot flags none, matching the complex vectors
        // `.ac` and `.noise` store.
        for variable in &mut variables {
            variable.is_real = false;
        }
    }

    let mut plot = Plot::new(plotname.clone(), plotname, flags);
    plot.variables = variables;
    plot.points = points;

    Ok(Some((
        RawPlot {
            title,
            date,
            command,
            plot,
        },
        header_end + payload_len,
    )))
}

/// Decodes `header.point_count` points of one value per variable.
///
/// The caller has checked that the payload of that shape fits in `bytes`.
fn decode_points(
    bytes: &[u8],
    start: usize,
    header: &Header,
    byte_order: BinaryByteOrder,
) -> Vec<Vec<Complex>> {
    let columns = header.variables.len();
    let mut points = Vec::with_capacity(header.point_count);
    let mut offset = start;
    for _ in 0..header.point_count {
        let mut row = Vec::with_capacity(columns);
        for _ in 0..columns {
            let real = read_double(bytes, offset, byte_order);
            offset += DOUBLE_BYTES;
            let value = if header.flags.is_complex() {
                let imaginary = read_double(bytes, offset, byte_order);
                offset += DOUBLE_BYTES;
                Complex::new(real, imaginary)
            } else {
                Complex::real(real)
            };
            row.push(value);
        }
        points.push(row);
    }
    points
}

/// Bytes one value occupies: `re` for a real plot, `re` and `im` for a complex
/// one (rawfile.c:223-259 writes them; rawfile.c:667-694 reads them back).
const fn value_width(flags: PlotFlags) -> usize {
    match flags {
        PlotFlags::Real => DOUBLE_BYTES,
        PlotFlags::Complex => 2 * DOUBLE_BYTES,
    }
}

/// Reads one stored `double`. The caller has checked the bounds.
fn read_double(bytes: &[u8], offset: usize, byte_order: BinaryByteOrder) -> Real {
    let mut buffer = [0_u8; DOUBLE_BYTES];
    buffer.copy_from_slice(&bytes[offset..offset + DOUBLE_BYTES]);
    byte_order.decode(buffer)
}

/// True when the slice holds nothing but ASCII whitespace.
fn is_blank(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_whitespace)
}

/// Finds the first `Values:` or `Binary:` line at or after `from`.
///
/// Returns the offset just past that line and which section it announces. The
/// key is matched the way the header parser matches it: everything before the
/// line's first `:` with ASCII whitespace trimmed.
fn find_section_line(bytes: &[u8], from: usize) -> Option<(usize, Section)> {
    let mut line_start = from;
    loop {
        let rest = &bytes[line_start..];
        let (line, line_end) = match rest.iter().position(|byte| *byte == b'\n') {
            Some(index) => (&rest[..index], line_start + index + 1),
            None => (rest, bytes.len()),
        };
        if let Some(colon) = line.iter().position(|byte| *byte == b':') {
            let key = line[..colon].trim_ascii();
            if key == b"Values" {
                return Some((line_end, Section::Values));
            }
            if key == b"Binary" {
                return Some((line_end, Section::Binary));
            }
        }
        if line_end >= bytes.len() {
            return None;
        }
        line_start = line_end;
    }
}

/// Renders every plot in the binary form, as `raw_write()` does when
/// `set filetype=binary` sends it down its `if (binary)` branch.
///
/// # Errors
///
/// [`crate::primitives::SpiceError::Numerical`] from [`RawFile::validate`].
pub(super) fn render(rawfile: &RawFile, byte_order: BinaryByteOrder) -> SpiceResult<Vec<u8>> {
    rawfile.validate()?;
    let mut bytes = Vec::new();
    for raw_plot in &rawfile.plots {
        let plot = &raw_plot.plot;
        // One header renderer serves both encodings, so they cannot drift apart.
        let mut header = String::new();
        render_header(&mut header, raw_plot);
        bytes.extend_from_slice(header.as_bytes());
        bytes.extend_from_slice(b"Binary:\n");
        for point in &plot.points {
            for (column, value) in point.iter().enumerate() {
                match plot.flags {
                    PlotFlags::Real => bytes.extend_from_slice(&byte_order.encode(value.re)),
                    PlotFlags::Complex => {
                        // `raw_write()` writes `(re, 0.0)` for a vector it has
                        // flagged real, exactly as the ASCII writer does.
                        let is_real = plot
                            .variables
                            .get(column)
                            .is_some_and(|variable| variable.is_real);
                        let imaginary = if is_real { 0.0 } else { value.im };
                        bytes.extend_from_slice(&byte_order.encode(value.re));
                        bytes.extend_from_slice(&byte_order.encode(imaginary));
                    }
                }
            }
        }
    }
    Ok(bytes)
}
