//! The distortion load functions of `src/spicelib/analysis/dloadfns.c`,
//! transcribed with C's exact operand order and grouping so that the
//! right-hand sides of the higher-order solves round as C's do.
//!
//! Each function returns the real (`n`/`v`) or imaginary (`i`) part of a
//! Taylor polynomial evaluated on the lower-order Volterra kernels; `x`, `y`,
//! `z` are the controls' kernel values (C's `r1h1x`, `i1h1x`, ...), and the
//! coefficients follow C's argument order `cxx, cyy, czz, cxy, cyz, cxz,
//! cxxx, cyyy, czzz, cxxy, cxxz, cxyy, cyyz, cxzz, cyzz, cxyz`.

use super::Taylor;
use crate::primitives::{Complex, Real};

/// The three controls' values in one kernel (zero for a missing control).
pub(super) type Values = [Complex; 3];

fn s2v2f1(c: Real, x: Complex, y: Complex) -> Real {
    c * (x.re * y.re - x.im * y.im)
}

fn s2i2f1(c: Real, x: Complex, y: Complex) -> Real {
    c * (x.re * y.im + x.im * y.re)
}

/// `S2v3F1` and `S2vF12` (the same expression): `x1`, `y1` first-order,
/// `x2`, `y2` the other kernel.
fn s2v3f1(c: Real, x1: Complex, y1: Complex, x2: Complex, y2: Complex) -> Real {
    c * (x1.re * y2.re - x1.im * y2.im + y1.re * x2.re - y1.im * x2.im)
}

/// `S2i3F1` and `S2iF12`.
fn s2i3f1(c: Real, x1: Complex, y1: Complex, x2: Complex, y2: Complex) -> Real {
    c * (x1.re * y2.im + x1.im * y2.re + y1.re * x2.im + y1.im * x2.re)
}

/// The kernels of a `2 f1 - f2` term: `H1(f1)`, `H1(-f2)`, `H2(f1, f1)` and
/// `H2(f1, -f2)` values of one pair of controls.
#[derive(Clone, Copy)]
struct Im3 {
    h1: Complex,
    h1m2: Complex,
    h11: Complex,
    h1m2_2: Complex,
}

#[allow(clippy::similar_names)]
fn s2v2f12(c: Real, x: Im3, y: Im3) -> Real {
    c * (2.
        * (x.h1.re * y.h1m2_2.re - x.h1.im * y.h1m2_2.im + y.h1.re * x.h1m2_2.re
            - y.h1.im * x.h1m2_2.im)
        + x.h1m2.re * y.h11.re
        - x.h1m2.im * y.h11.im
        + y.h1m2.re * x.h11.re
        - y.h1m2.im * x.h11.im)
}

#[allow(clippy::similar_names)]
fn s2i2f12(c: Real, x: Im3, y: Im3) -> Real {
    c * (2.
        * (x.h1.re * y.h1m2_2.im
            + x.h1.im * y.h1m2_2.re
            + y.h1.re * x.h1m2_2.im
            + y.h1.im * x.h1m2_2.re)
        + x.h1m2.re * y.h11.im
        + x.h1m2.im * y.h11.re
        + y.h1m2.re * x.h11.im
        + y.h1m2.im * x.h11.re)
}

fn s3v3f1(c: Real, x: Complex, y: Complex, z: Complex) -> Real {
    c * ((x.re * y.re - x.im * y.im) * z.re - (x.im * y.re + x.re * y.im) * z.im)
}

fn s3i3f1(c: Real, x: Complex, y: Complex, z: Complex) -> Real {
    c * ((x.re * y.re - x.im * y.im) * z.im + (x.im * y.re + x.re * y.im) * z.re)
}

/// `S3v2F12`: `x`, `y`, `z` first-order, `x2`, `y2`, `z2` at `-f2`.
#[allow(clippy::many_single_char_names)]
fn s3v2f12(c: Real, [x, y, z]: Values, [x2, y2, z2]: Values) -> Real {
    c * ((x.re * y.re - x.im * y.im) * z2.re - (x.im * y.re + x.re * y.im) * z2.im
        + (x.re * z.re - x.im * z.im) * y2.re
        - (x.im * z.re + x.re * z.im) * y2.im
        + (z.re * y.re - z.im * y.im) * x2.re
        - (z.im * y.re + z.re * y.im) * x2.im)
}

/// `S3i2F12`.
#[allow(clippy::many_single_char_names)]
fn s3i2f12(c: Real, [x, y, z]: Values, [x2, y2, z2]: Values) -> Real {
    c * ((x.re * y.re - x.im * y.im) * z2.im
        + (x.im * y.re + x.re * y.im) * z2.re
        + (x.re * z.re - x.im * z.im) * y2.im
        + (x.im * z.re + x.re * z.im) * y2.re
        + (z.re * y.re - z.im * y.im) * x2.im
        + (z.im * y.re + z.re * y.im) * x2.re)
}

/// `DFn2F1`/`DFi2F1` (and `D1n2F1`/`D1i2F1`, equal for one control).
pub(super) fn two_f1(t: &Taylor, [x, y, z]: Values) -> Complex {
    let re = s2v2f1(t.xx, x, x)
        + s2v2f1(t.yy, y, y)
        + s2v2f1(t.zz, z, z)
        + s2v2f1(t.xy, x, y)
        + s2v2f1(t.yz, y, z)
        + s2v2f1(t.xz, x, z);
    let im = s2i2f1(t.xx, x, x)
        + s2i2f1(t.yy, y, y)
        + s2i2f1(t.zz, z, z)
        + s2i2f1(t.xy, x, y)
        + s2i2f1(t.yz, y, z)
        + s2i2f1(t.xz, x, z);
    Complex::new(re, im)
}

/// `DFn3F1`/`DFi3F1` with `h1` = `H1(f1)` and `h2` = `H2(f1, f1)` values,
/// grouped into C's four `temp +=` statements (which differ between the
/// real and imaginary parts).
pub(super) fn three_f1(t: &Taylor, h1: Values, h2: Values) -> Complex {
    let [x, y, z] = h1;
    let [x2, y2, z2] = h2;
    let mut re =
        s2v3f1(t.xx, x, x, x2, x2) + s2v3f1(t.yy, y, y, y2, y2) + s2v3f1(t.zz, z, z, z2, z2);
    re += s2v3f1(t.xy, x, y, x2, y2)
        + s2v3f1(t.yz, y, z, y2, z2)
        + s2v3f1(t.xz, x, z, x2, z2)
        + s3v3f1(t.xxx, x, x, x);
    re += s3v3f1(t.yyy, y, y, y)
        + s3v3f1(t.zzz, z, z, z)
        + s3v3f1(t.xxy, x, x, y)
        + s3v3f1(t.xxz, x, x, z)
        + s3v3f1(t.xyy, x, y, y);
    re += s3v3f1(t.yyz, y, y, z)
        + s3v3f1(t.xzz, x, z, z)
        + s3v3f1(t.yzz, y, z, z)
        + s3v3f1(t.xyz, x, y, z);
    let mut im = s2i3f1(t.xx, x, x, x2, x2)
        + s2i3f1(t.yy, y, y, y2, y2)
        + s2i3f1(t.zz, z, z, z2, z2)
        + s2i3f1(t.xy, x, y, x2, y2);
    im += s2i3f1(t.yz, y, z, y2, z2)
        + s2i3f1(t.xz, x, z, x2, z2)
        + s3i3f1(t.xxx, x, x, x)
        + s3i3f1(t.yyy, y, y, y);
    im += s3i3f1(t.zzz, z, z, z)
        + s3i3f1(t.xxy, x, x, y)
        + s3i3f1(t.xxz, x, x, z)
        + s3i3f1(t.xyy, x, y, y);
    im += s3i3f1(t.yyz, y, y, z)
        + s3i3f1(t.xzz, x, z, z)
        + s3i3f1(t.yzz, y, z, z)
        + s3i3f1(t.xyz, x, y, z);
    Complex::new(re, im)
}

/// `DFnF12`/`DFiF12` with `h1` = `H1(f1)` and `h2` = `H1(+-f2)` values.
pub(super) fn f12(t: &Taylor, h1: Values, h2: Values) -> Complex {
    let [x, y, z] = h1;
    let [x2, y2, z2] = h2;
    let mut re =
        s2v3f1(t.xx, x, x, x2, x2) + s2v3f1(t.yy, y, y, y2, y2) + s2v3f1(t.zz, z, z, z2, z2);
    re += s2v3f1(t.xy, x, y, x2, y2) + s2v3f1(t.yz, y, z, y2, z2) + s2v3f1(t.xz, x, z, x2, z2);
    let mut im =
        s2i3f1(t.xx, x, x, x2, x2) + s2i3f1(t.yy, y, y, y2, y2) + s2i3f1(t.zz, z, z, z2, z2);
    im += s2i3f1(t.xy, x, y, x2, y2) + s2i3f1(t.yz, y, z, y2, z2) + s2i3f1(t.xz, x, z, x2, z2);
    Complex::new(0.5 * re, im * 0.5)
}

/// `DFn2F12`/`DFi2F12` (the `DpassStr` functions) with the `H1(f1)`,
/// `H1(-f2)`, `H2(f1, f1)` and `H2(f1, -f2)` values.
#[allow(clippy::similar_names)]
pub(super) fn two_f1_minus_f2(
    t: &Taylor,
    h1: Values,
    h1m2: Values,
    h11: Values,
    h1m2_2: Values,
) -> Complex {
    let k = |i: usize| Im3 {
        h1: h1[i],
        h1m2: h1m2[i],
        h11: h11[i],
        h1m2_2: h1m2_2[i],
    };
    let (x, y, z) = (k(0), k(1), k(2));
    let [a, b, c] = h1;
    let cube = |u: Complex, v: Complex, w: Complex| [u, v, w];
    let [a2, b2, c2] = h1m2;
    let mut re = s2v2f12(t.xx, x, x);
    re += s2v2f12(t.yy, y, y);
    re += s2v2f12(t.zz, z, z);
    re += s2v2f12(t.xy, x, y);
    re += s2v2f12(t.yz, y, z);
    re += s2v2f12(t.xz, x, z);
    re += s3v2f12(t.xxx, cube(a, a, a), cube(a2, a2, a2))
        + s3v2f12(t.yyy, cube(b, b, b), cube(b2, b2, b2));
    re += s3v2f12(t.zzz, cube(c, c, c), cube(c2, c2, c2))
        + s3v2f12(t.xxy, cube(a, a, b), cube(a2, a2, b2));
    re += s3v2f12(t.xxz, cube(a, a, c), cube(a2, a2, c2))
        + s3v2f12(t.xyy, cube(a, b, b), cube(a2, b2, b2));
    re += s3v2f12(t.yyz, cube(b, b, c), cube(b2, b2, c2))
        + s3v2f12(t.xzz, cube(a, c, c), cube(a2, c2, c2));
    re += s3v2f12(t.yzz, cube(b, c, c), cube(b2, c2, c2))
        + s3v2f12(t.xyz, cube(a, b, c), cube(a2, b2, c2));
    let mut im = s2i2f12(t.xx, x, x);
    im += s2i2f12(t.yy, y, y);
    im += s2i2f12(t.zz, z, z);
    im += s2i2f12(t.xy, x, y);
    im += s2i2f12(t.yz, y, z);
    im += s2i2f12(t.xz, x, z);
    im += s3i2f12(t.xxx, cube(a, a, a), cube(a2, a2, a2));
    im += s3i2f12(t.yyy, cube(b, b, b), cube(b2, b2, b2))
        + s3i2f12(t.zzz, cube(c, c, c), cube(c2, c2, c2));
    im += s3i2f12(t.xxy, cube(a, a, b), cube(a2, a2, b2))
        + s3i2f12(t.xxz, cube(a, a, c), cube(a2, a2, c2));
    im += s3i2f12(t.xyy, cube(a, b, b), cube(a2, b2, b2))
        + s3i2f12(t.yyz, cube(b, b, c), cube(b2, b2, c2));
    im += s3i2f12(t.xzz, cube(a, c, c), cube(a2, c2, c2));
    im += s3i2f12(t.yzz, cube(b, c, c), cube(b2, c2, c2))
        + s3i2f12(t.xyz, cube(a, b, c), cube(a2, b2, c2));
    Complex::new(re / 3., im / 3.)
}
