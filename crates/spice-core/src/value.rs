//! SPICE numeric literals, scale factors, and the complex type.
//!
//! # Scale factors
//!
//! Ported from `INPevaluate()` in `src/spicelib/parser/inpeval.c` (the switch on
//! `*here` around line 143) and the scale-factor scanning in
//! `src/frontend/inpcom.c` around line 3098.
//!
//! The table is **not** SI, and the surprises are load-bearing for compatibility:
//!
//! | Suffix | Multiplier | Note |
//! | --- | --- | --- |
//! | `T` | 1e12 | |
//! | `G` | 1e9 | |
//! | `MEG` | 1e6 | |
//! | `K` | 1e3 | |
//! | `MIL` | 25.4e-6 | thousandths of an inch |
//! | `M` | 1e-3 | **milli**, not mega |
//! | `L` | 1e-3 | accepted by `INPevaluate()` |
//! | `U` | 1e-6 | |
//! | `N` | 1e-9 | |
//! | `P` | 1e-12 | |
//! | `F` | 1e-15 | |
//! | `A` | 1e-18 | accepted by `INPevaluate()` |
//!
//! An unrecognised trailing letter does not change the value: `5V` is `5`, and
//! `1kohm` is `1000`. Anything else after the literal — `1k2` — is rejected by
//! [`parse_spice_number`]; see the RKM note on that function.

use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// The floating point type used throughout the port.
///
/// ngspice mixes `double` and `float`; the port uses `f64` everywhere and
/// records the places where the C code downcasts as a divergence risk.
pub type Real = f64;

/// Compares two reals with a relative tolerance, falling back to an absolute one
/// near zero.
#[must_use]
pub fn approx_eq(a: Real, b: Real, relative_tolerance: Real) -> bool {
    if a == b {
        return true;
    }
    if !a.is_finite() || !b.is_finite() {
        return false;
    }
    let scale = a.abs().max(b.abs());
    (a - b).abs() <= relative_tolerance * scale.max(1.0)
}

/// A complex number, mirroring ngspice's `complex` struct
/// (`src/include/ngspice/ngspice.h`), which is a two-element `double` array.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Complex {
    /// Real part.
    pub re: Real,
    /// Imaginary part.
    pub im: Real,
}

impl Complex {
    /// `0 + 0i`.
    pub const ZERO: Self = Self { re: 0.0, im: 0.0 };

    /// Builds a complex number.
    #[must_use]
    pub const fn new(re: Real, im: Real) -> Self {
        Self { re, im }
    }

    /// A purely real value.
    #[must_use]
    pub const fn real(re: Real) -> Self {
        Self { re, im: 0.0 }
    }

    /// A purely imaginary value.
    #[must_use]
    pub const fn imaginary(im: Real) -> Self {
        Self { re: 0.0, im }
    }

    /// `sqrt(re^2 + im^2)`.
    #[must_use]
    pub fn magnitude(self) -> Real {
        self.re.hypot(self.im)
    }

    /// The argument in radians.
    #[must_use]
    pub fn phase(self) -> Real {
        self.im.atan2(self.re)
    }

    /// The complex conjugate.
    #[must_use]
    pub const fn conj(self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
        }
    }

    /// True when both parts are finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }

    /// True when the imaginary part is zero.
    #[must_use]
    pub fn is_real(self) -> bool {
        self.im == 0.0
    }
}

impl From<Real> for Complex {
    fn from(re: Real) -> Self {
        Self::real(re)
    }
}

impl Add for Complex {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.re + rhs.re, self.im + rhs.im)
    }
}

impl Sub for Complex {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::new(self.re - rhs.re, self.im - rhs.im)
    }
}

impl Mul for Complex {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        Self::new(
            self.re * rhs.re - self.im * rhs.im,
            self.re * rhs.im + self.im * rhs.re,
        )
    }
}

impl Div for Complex {
    type Output = Self;

    fn div(self, rhs: Self) -> Self {
        let denominator = rhs.re * rhs.re + rhs.im * rhs.im;
        Self::new(
            (self.re * rhs.re + self.im * rhs.im) / denominator,
            (self.im * rhs.re - self.re * rhs.im) / denominator,
        )
    }
}

impl Neg for Complex {
    type Output = Self;

    fn neg(self) -> Self {
        Self::new(-self.re, -self.im)
    }
}

impl fmt::Display for Complex {
    /// ngspice's ASCII rawfile spelling: `re` when real, `re,im` otherwise.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.im == 0.0 {
            write!(f, "{}", format_spice_number(self.re))
        } else {
            write!(
                f,
                "{},{}",
                format_spice_number(self.re),
                format_spice_number(self.im)
            )
        }
    }
}

/// A numeric literal plus how many bytes of the input it used.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParsedNumber {
    /// The value, with the scale factor applied.
    pub value: Real,
    /// Bytes of the input consumed, including the scale suffix.
    pub consumed: usize,
}

/// Multiplier and byte-width of a scale suffix.
///
/// `letter` is the first letter after the numeric part, `rest` the text that
/// follows it. `MEG` and `MIL` are three bytes wide, everything else one.
fn scale_suffix(letter: char, rest: &str) -> (Real, usize) {
    fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
        text.len() >= prefix.len()
            && text.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
    }

    match letter.to_ascii_lowercase() {
        't' => (1e12, 1),
        'g' => (1e9, 1),
        'k' => (1e3, 1),
        'u' => (1e-6, 1),
        'n' => (1e-9, 1),
        'p' => (1e-12, 1),
        'f' => (1e-15, 1),
        'a' => (1e-18, 1),
        'l' => (1e-3, 1),
        'm' => {
            if starts_with_ignore_case(rest, "eg") {
                (1e6, 3)
            } else if starts_with_ignore_case(rest, "il") {
                (25.4e-6, 3)
            } else {
                (1e-3, 1)
            }
        }
        _ => (1.0, 0),
    }
}

/// Parses a literal from the start of `input`, reporting bytes consumed.
///
/// Does not skip leading whitespace: the literal must begin at byte 0. Returns
/// `None` when no numeric part is present. The numeric part is
/// `[+-]?(digits[.digits?]|.digits)([eE][+-]?digits)?`, optionally followed by a
/// scale suffix.
///
/// # Divergence from `INPevaluate()`
///
/// `INPevaluate()` applies the suffix multiplier but leaves `MEG`/`MIL`
/// unconsumed in its output pointer, because its callers do the skipping.
/// This function consumes the whole recognised suffix, so `consumed` is the
/// full width. The numeric result is identical.
#[must_use]
pub fn parse_spice_number_prefix(input: &str) -> Option<ParsedNumber> {
    let bytes = input.as_bytes();
    let mut i = 0;

    if matches!(bytes.first(), Some(b'+' | b'-')) {
        i += 1;
    }

    let mut mantissa_digits = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
        mantissa_digits += 1;
    }
    if bytes.get(i) == Some(&b'.') {
        i += 1;
        let fraction_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        mantissa_digits += i - fraction_start;
    }
    if mantissa_digits == 0 {
        return None;
    }

    if bytes
        .get(i)
        .is_some_and(|byte| byte.eq_ignore_ascii_case(&b'e'))
    {
        let mut exponent_end = i + 1;
        if matches!(bytes.get(exponent_end), Some(b'+' | b'-')) {
            exponent_end += 1;
        }
        let exponent_digits_start = exponent_end;
        while exponent_end < bytes.len() && bytes[exponent_end].is_ascii_digit() {
            exponent_end += 1;
        }
        // `1e` with nothing after it is a mantissa of `1` followed by junk, not
        // an exponent; leave it for the caller.
        if exponent_end > exponent_digits_start {
            i = exponent_end;
        }
    }

    // The sign is part of the slice, so a negative literal keeps its sign.
    let mantissa: Real = input[..i].parse().ok()?;
    let (factor, suffix_width) = match input[i..].chars().next() {
        Some(letter) => scale_suffix(letter, &input[i + letter.len_utf8()..]),
        None => (1.0, 0),
    };

    Some(ParsedNumber {
        value: mantissa * factor,
        consumed: i + suffix_width,
    })
}

/// Parses a complete SPICE numeric literal, scale factor included.
///
/// Trailing letters are treated as a unit name and ignored, as ngspice does:
/// `5V` is `5`, `1kohm` is `1000`. Leading and trailing whitespace is allowed.
///
/// # Not yet ported
///
/// The RKM-style literals accepted by `inp2r.c`, `inp2c.c` and `inp2l.c` — `4k7`
/// for `4.7k`, `2M2` for `2.2M` — are **not** handled, and this function returns
/// `None` for them. See `docs/port/ARCHITECTURE.md` for the divergence record.
#[must_use]
pub fn parse_spice_number(input: &str) -> Option<Real> {
    let trimmed = input.trim_start();
    let parsed = parse_spice_number_prefix(trimmed)?;
    let rest = trimmed[parsed.consumed..].trim();
    if rest.is_empty() || rest.chars().all(|c| c.is_ascii_alphabetic()) {
        Some(parsed.value)
    } else {
        None
    }
}

/// Formats a real the way ngspice writes ASCII rawfiles: `%-.15e`, always with a
/// signed two-digit exponent.
///
/// `5.0` becomes `5.000000000000000e+00` and `-2.5e-3` becomes
/// `-2.500000000000000e-03`, matching `src/frontend/rawfile.c`.
#[must_use]
pub fn format_spice_number(value: Real) -> String {
    let text = format!("{value:.15e}");
    let Some((mantissa, exponent)) = text.split_once('e') else {
        // `NaN`, `inf` and `-inf` have no exponent part.
        return text;
    };
    match exponent.parse::<i32>() {
        Ok(exponent) => format!(
            "{mantissa}e{}{:02}",
            if exponent < 0 { '-' } else { '+' },
            exponent.unsigned_abs()
        ),
        Err(_) => text,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Complex, approx_eq, format_spice_number, parse_spice_number, parse_spice_number_prefix,
    };

    #[test]
    fn parses_plain_numbers() {
        assert_eq!(parse_spice_number("1"), Some(1.0));
        assert_eq!(parse_spice_number("1.5"), Some(1.5));
        assert_eq!(parse_spice_number(".5"), Some(0.5));
        assert_eq!(parse_spice_number("-2.5e-3"), Some(-2.5e-3));
        assert_eq!(parse_spice_number("+1E3"), Some(1000.0));
        assert_eq!(parse_spice_number("  42  "), Some(42.0));
    }

    #[test]
    fn scale_factors_match_inpevaluate() {
        assert_eq!(parse_spice_number("1k"), Some(1000.0));
        assert_eq!(parse_spice_number("1K"), Some(1000.0));
        assert_eq!(parse_spice_number("1M"), Some(1e-3), "M is milli, not mega");
        assert_eq!(parse_spice_number("1m"), Some(1e-3));
        assert_eq!(parse_spice_number("1meg"), Some(1e6));
        assert_eq!(parse_spice_number("1MEG"), Some(1e6));
        assert_eq!(parse_spice_number("1MeG"), Some(1e6));
        assert_eq!(parse_spice_number("1mil"), Some(25.4e-6));
        assert_eq!(parse_spice_number("1MIL"), Some(25.4e-6));
        assert_eq!(parse_spice_number("1u"), Some(1e-6));
        assert_eq!(parse_spice_number("1n"), Some(1e-9));
        assert_eq!(parse_spice_number("1p"), Some(1e-12));
        assert_eq!(parse_spice_number("1f"), Some(1e-15));
        assert_eq!(parse_spice_number("1a"), Some(1e-18));
        assert_eq!(parse_spice_number("1t"), Some(1e12));
        assert_eq!(parse_spice_number("1g"), Some(1e9));
        assert_eq!(parse_spice_number("1l"), Some(1e-3));
    }

    #[test]
    fn trailing_unit_names_are_ignored() {
        assert_eq!(parse_spice_number("5V"), Some(5.0));
        assert_eq!(parse_spice_number("1kohm"), Some(1000.0));
        assert_eq!(parse_spice_number("1000F"), Some(1e-15 * 1000.0));
    }

    #[test]
    fn rejects_non_literals() {
        assert_eq!(parse_spice_number(""), None);
        assert_eq!(parse_spice_number("abc"), None);
        assert_eq!(parse_spice_number("-"), None);
        assert_eq!(parse_spice_number("."), None);
        assert_eq!(parse_spice_number("2N2222"), None, "a BJT part number");
    }

    #[test]
    fn rkm_literals_are_not_yet_supported() {
        // Pins the divergence documented in `docs/port/ARCHITECTURE.md`.
        // `INPevaluateRKM_R()` in `src/spicelib/parser/inpeval.c` accepts these.
        assert_eq!(parse_spice_number("4k7"), None);
        assert_eq!(parse_spice_number("2M2"), None);
    }

    #[test]
    fn prefix_reports_bytes_consumed() {
        let parsed = parse_spice_number_prefix("1megohm").unwrap();
        assert_eq!(parsed.consumed, 4);
        assert_eq!(parsed.value, 1e6);

        let parsed = parse_spice_number_prefix("2.5uF").unwrap();
        assert_eq!(parsed.consumed, 4);
        // `INPevaluate()` computes `mantissa * pow(10, expo)`; it does not parse
        // the scaled literal directly, so the result is this product and not
        // necessarily the nearest double to `2.5e-6`.
        assert_eq!(parsed.value, 2.5 * 1e-6);

        let parsed = parse_spice_number_prefix("1e3").unwrap();
        assert_eq!(parsed.consumed, 3);
        assert_eq!(parsed.value, 1000.0);

        let parsed = parse_spice_number_prefix("-2.5e-3").unwrap();
        assert_eq!(parsed.consumed, 7);
        assert_eq!(parsed.value, -2.5e-3);
    }

    #[test]
    fn formats_like_the_rawfile_writer() {
        assert_eq!(format_spice_number(5.0), "5.000000000000000e+00");
        assert_eq!(format_spice_number(2.5), "2.500000000000000e+00");
        assert_eq!(format_spice_number(-2.5e-3), "-2.500000000000000e-03");
        assert_eq!(format_spice_number(0.0), "0.000000000000000e+00");
        assert_eq!(format_spice_number(1e100), "1.000000000000000e+100");
        assert_eq!(format_spice_number(f64::NAN), "NaN");
    }

    #[test]
    fn formats_and_parses_round_trip() {
        for value in [0.0, 1.0, -1.0, 2.5e-3, 1e12, -3.75e-9] {
            let formatted = format_spice_number(value);
            let parsed = parse_spice_number(&formatted).expect("formatted value must parse");
            assert!(approx_eq(value, parsed, 1e-15), "{formatted}");
        }
    }

    #[test]
    fn complex_arithmetic() {
        let a = Complex::new(3.0, 4.0);
        let b = Complex::new(1.0, -2.0);
        assert_eq!(a + b, Complex::new(4.0, 2.0));
        assert_eq!(a - b, Complex::new(2.0, 6.0));
        assert_eq!(a * b, Complex::new(11.0, -2.0));
        assert_eq!(a.conj(), Complex::new(3.0, -4.0));
        assert_eq!(a.magnitude(), 5.0);
        assert_eq!(Complex::real(2.0).to_string(), "2.000000000000000e+00");
        assert_eq!(
            b.to_string(),
            "1.000000000000000e+00,-2.000000000000000e+00"
        );
    }
}
