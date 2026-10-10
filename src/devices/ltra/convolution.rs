//! Impulse-response integrals, interpolation and convolution coefficients of
//! the lossy line (C `src/spicelib/devices/ltra/ltramisc.c`).
//!
//! The line equations (Roychowdhury and Pederson's formulation, as LTRA
//! implements it) convolve the port voltages and currents with three
//! impulse responses `h1'`, `h2` and `h3'`. Over the accepted time points the
//! waveforms are taken piecewise linear, so each convolution becomes a sum of
//! the past samples weighted by coefficients built from repeated integrals of
//! the impulse responses; the coefficient of the newest (unknown) sample goes
//! into the matrix, the rest into the right-hand side.
//!
//! Every routine follows its C counterpart operation by operation, including
//! C's quirks (noted where they matter), so that the port reproduces ngspice's
//! coefficients to rounding. The modified Bessel functions are the polynomial
//! approximations of Abramowitz and Stegun 9.8.1-9.8.4 that `ltramisc.c` uses
//! (`bessI0`, `bessI1`, `bessI1xOverX`); `erfc` is a separate implementation
//! (see [`erfc`]).

use std::f64::consts::PI;

use crate::primitives::Real;

/// `LTRAquadInterp`: coefficients `(c1, c2, c3)` of the quadratic through
/// `(t1, t2, t3)` evaluated at `t`, or `None` for coincident times (C returns
/// 1 and leaves the coefficients unset).
#[must_use]
pub fn quad_interp(t: Real, t1: Real, t2: Real, t3: Real) -> Option<[Real; 3]> {
    if t == t1 {
        return Some([1., 0., 0.]);
    }
    if t == t2 {
        return Some([0., 1., 0.]);
    }
    if t == t3 {
        return Some([0., 0., 1.]);
    }
    if (t2 - t1) == 0. || (t3 - t2) == 0. || (t1 - t3) == 0. {
        return None;
    }
    let mut f1 = (t - t2) * (t - t3);
    let mut f2 = (t - t1) * (t - t3);
    let mut f3 = (t - t1) * (t - t2);
    // C's further zero checks are unreachable after the test above.
    f1 /= t1 - t2;
    f2 /= t2 - t1;
    f2 /= t2 - t3;
    f3 /= t2 - t3;
    f1 /= t1 - t3;
    f3 /= t1 - t3;
    Some([f1, f2, f3])
}

/// `LTRAlinInterp`: coefficients `(c1, c2)` of the line through `(t1, t2)` at
/// `t`, or `None` for `t1 == t2`.
#[must_use]
pub fn lin_interp(t: Real, t1: Real, t2: Real) -> Option<[Real; 2]> {
    if t1 == t2 {
        return None;
    }
    if t == t1 {
        return Some([1., 0.]);
    }
    if t == t2 {
        return Some([0., 1.]);
    }
    let temp = (t - t1) / (t2 - t1);
    Some([1. - temp, temp])
}

/// `intlinfunc`: `int_lo^hi h` for `h` linear through `(t1, lovalue)` and
/// `(t2, hivalue)`.
fn intlinfunc(
    lolimit: Real,
    hilimit: Real,
    lovalue: Real,
    hivalue: Real,
    t1: Real,
    t2: Real,
) -> Real {
    let width = t2 - t1;
    if width == 0. {
        return 0.;
    }
    let m = (hivalue - lovalue) / width;
    (hilimit - lolimit) * lovalue
        + 0.5 * m * ((hilimit - t1) * (hilimit - t1) - (lolimit - t1) * (lolimit - t1))
}

/// `twiceintlinfunc`: `int_lo^hi int_other^tau h` for the same linear `h`.
#[allow(clippy::too_many_arguments)]
fn twiceintlinfunc(
    lolimit: Real,
    hilimit: Real,
    otherlolimit: Real,
    lovalue: Real,
    hivalue: Real,
    t1: Real,
    t2: Real,
) -> Real {
    let width = t2 - t1;
    if width == 0. {
        return 0.;
    }
    let m = (hivalue - lovalue) / width;
    let temp1 = hilimit - t1;
    let temp2 = lolimit - t1;
    let temp3 = otherlolimit - t1;
    let mut dummy = lovalue
        * ((hilimit - otherlolimit) * (hilimit - otherlolimit)
            - (lolimit - otherlolimit) * (lolimit - otherlolimit));
    dummy += m
        * ((temp1 * temp1 * temp1 - temp2 * temp2 * temp2) / 3.0
            - temp3 * temp3 * (hilimit - lolimit));
    dummy * 0.5
}

/// `bessI0` (Abramowitz and Stegun 9.8.1, 9.8.2).
#[must_use]
pub fn bess_i0(x: Real) -> Real {
    let ax = x.abs();
    if ax < 3.75 {
        let mut y = x / 3.75;
        y *= y;
        1.0 + y
            * (3.5156229
                + y * (3.0899424
                    + y * (1.2067492 + y * (0.2659732 + y * (0.360768e-1 + y * 0.45813e-2)))))
    } else {
        let y = 3.75 / ax;
        (ax.exp() / ax.sqrt())
            * (0.39894228
                + y * (0.1328592e-1
                    + y * (0.225319e-2
                        + y * (-0.157565e-2
                            + y * (0.916281e-2
                                + y * (-0.2057706e-1
                                    + y * (0.2635537e-1
                                        + y * (-0.1647633e-1 + y * 0.392377e-2))))))))
    }
}

/// The large-argument series shared by `bessI1` and `bessI1xOverX`.
fn bess_i1_tail(y: Real) -> Real {
    let ans = 0.2282967e-1 + y * (-0.2895312e-1 + y * (0.1787654e-1 - y * 0.420059e-2));
    0.39894228
        + y * (-0.3988024e-1
            + y * (-0.362018e-2 + y * (0.163801e-2 + y * (-0.1031555e-1 + y * ans))))
}

/// `bessI1` (Abramowitz and Stegun 9.8.3, 9.8.4).
#[must_use]
pub fn bess_i1(x: Real) -> Real {
    let ax = x.abs();
    let ans = if ax < 3.75 {
        let mut y = x / 3.75;
        y *= y;
        ax * (0.5
            + y * (0.87890594
                + y * (0.51498869
                    + y * (0.15084934 + y * (0.2658733e-1 + y * (0.301532e-2 + y * 0.32411e-3))))))
    } else {
        let y = 3.75 / ax;
        bess_i1_tail(y) * (ax.exp() / ax.sqrt())
    };
    if x < 0.0 { -ans } else { ans }
}

/// `bessI1xOverX`: `I1(x) / x` for `x >= 0` (C uses `|x|`).
#[must_use]
pub fn bess_i1_x_over_x(x: Real) -> Real {
    let ax = x.abs();
    if ax < 3.75 {
        let mut y = x / 3.75;
        y *= y;
        0.5 + y
            * (0.87890594
                + y * (0.51498869
                    + y * (0.15084934 + y * (0.2658733e-1 + y * (0.301532e-2 + y * 0.32411e-3)))))
    } else {
        let y = 3.75 / ax;
        bess_i1_tail(y) * (ax.exp() / (ax * ax.sqrt()))
    }
}

/// `LTRArlcH2Func`: `h2(t)` of the RLC line, zero before `T`.
#[must_use]
pub fn rlc_h2(time: Real, t: Real, alpha: Real, beta: Real) -> Real {
    if alpha == 0.0 || time < t {
        return 0.0;
    }
    let besselarg = if time != t {
        alpha * (time * time - t * t).sqrt()
    } else {
        0.0
    };
    let exparg = -beta * time;
    alpha * alpha * t * exparg.exp() * bess_i1_x_over_x(besselarg)
}

/// `LTRAstraightLineCheck`: whether three points lie close enough to a
/// straight line, by comparing the triangle they span with the trapezoid area
/// under them (`area * reltol + abstol > triangle`).
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn straight_line_check(
    x1: Real,
    y1: Real,
    x2: Real,
    y2: Real,
    x3: Real,
    y3: Real,
    reltol: Real,
    abstol: Real,
) -> bool {
    let quad1 = (y2.abs() + y1.abs()) * 0.5 * (x2 - x1).abs();
    let quad2 = (y3.abs() + y2.abs()) * 0.5 * (x3 - x2).abs();
    let quad3 = (y3.abs() + y1.abs()) * 0.5 * (x3 - x1).abs();
    let triangle = (quad3 - quad1 - quad2).abs();
    let area = quad1 + quad2;
    area * reltol + abstol > triangle
}

/// The RLC line's `LTRAmaxSafeStep` (`ltratemp.c`): bisect `[TD, 10 TD]`
/// until `h2` is close enough to a straight line over `[TD, x]` (at most 51
/// halvings), and return `x - TD`.
///
/// C evaluates the straight-line test of `h2` twice (its sum with the
/// intended `h3'` test repeats the `h2` arguments), so only `h2` decides; the
/// port keeps that.
#[must_use]
pub fn rlc_max_safe_step(td: Real, alpha: Real, beta: Real, reltol: Real, abstol: Real) -> Real {
    let mut xbig = td + 9. * td;
    let xsmall = td;
    let mut xmid = 0.5 * (xbig + xsmall);
    let y1small = rlc_h2(xsmall, td, alpha, beta);
    let mut iters = 0;
    loop {
        iters += 1;
        let y1big = rlc_h2(xbig, td, alpha, beta);
        let y1mid = rlc_h2(xmid, td, alpha, beta);
        let done = straight_line_check(xbig, y1big, xmid, y1mid, xsmall, y1small, reltol, abstol);
        if done || iters > 50 {
            break;
        }
        xbig = xmid;
        xmid = 0.5 * (xbig + xsmall);
    }
    xbig - td
}

/// The complementary error function.
///
/// C's `ltramisc.c` calls the platform `erfc`; the port has no `libm`
/// dependency, so this evaluates the Maclaurin series of `erf` below 2 and
/// the classic continued fraction
/// `erfc(x) = exp(-x^2) / sqrt(pi) / (x + (1/2) / (x + 1 / (x + (3/2) / ...)))`
/// (Lentz's algorithm) above. Its relative error is below `1e-12` over the
/// range the line uses (`0 <= x < 10`, tested against reference values), far
/// inside the simulation tolerances, but it is not bit-identical to `libm`.
#[must_use]
pub fn erfc(x: Real) -> Real {
    if x.is_nan() {
        return x;
    }
    if x < 0. {
        return 2. - erfc(-x);
    }
    if x < 2. {
        // erf(x) = 2/sqrt(pi) sum (-1)^n x^(2n+1) / (n! (2n+1)).
        let x2 = x * x;
        let mut term = x;
        let mut sum = x;
        let mut n = 0.;
        loop {
            n += 1.;
            term *= -x2 / n;
            let add = term / (2. * n + 1.);
            sum += add;
            if add.abs() <= 1e-17 * sum.abs() || n > 200. {
                break;
            }
        }
        return 1. - 2. / PI.sqrt() * sum;
    }
    if x > 27. {
        return 0.;
    }
    // Lentz: f = x + a1/(x + a2/(x + ...)), a_n = n/2.
    let tiny = 1e-300;
    let mut f = x;
    let mut c = f;
    let mut d = 0.;
    for n in 1..=500 {
        let a = Real::from(n) / 2.;
        d = x + a * d;
        if d == 0. {
            d = tiny;
        }
        c = x + a / c;
        if c == 0. {
            c = tiny;
        }
        d = 1. / d;
        let delta = c * d;
        f *= delta;
        if (delta - 1.).abs() < 1e-16 {
            break;
        }
    }
    (-x * x).exp() / (PI.sqrt() * f)
}

/// The coefficients of one convolution step: the factor of the newest
/// (unknown) sample and the factors of the accepted samples `1..=index`
/// (entry 0 unused, as in C's lists).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Coefficients {
    /// `LTRAh1dashFirstCoeff`, `LTRAh2FirstCoeff`, `LTRAh3dashFirstCoeff`.
    pub first: [Real; 3],
    /// `LTRAh1dashCoeffs`.
    pub h1dash: Vec<Real>,
    /// `LTRAh2Coeffs`.
    pub h2: Vec<Real>,
    /// `LTRAh3dashCoeffs`.
    pub h3dash: Vec<Real>,
    /// `LTRAauxIndex`: the last accepted sample at least `TD` before the
    /// current time (RLC only; the `h2`/`h3'` sums stop there).
    pub aux_index: usize,
}

impl Coefficients {
    fn with_len(len: usize) -> Self {
        Self {
            first: [0.; 3],
            h1dash: vec![0.; len],
            h2: vec![0.; len],
            h3dash: vec![0.; len],
            aux_index: 0,
        }
    }
}

/// `LTRArcCoeffsSetup` (the `L = G = 0` line): coefficients at `curtime` over
/// the accepted times `times[0..=index]`, with C's chopping tolerance
/// `reltol` (C's `LTRAchopReltol`, always 0 from a deck).
#[must_use]
pub fn rc_coefficients(
    cbyr: Real,
    rclsqr: Real,
    curtime: Real,
    times: &[Real],
    reltol: Real,
) -> Coefficients {
    let index = times.len() - 1;
    let mut out = Coefficients::with_len(times.len());
    let (mut doh1, mut doh2, mut doh3) = (true, true, true);

    let mut delta1 = curtime - times[index];
    let mut hilimit1 = delta1;

    let mut h1hivalue1 = (4. * cbyr * hilimit1 / PI).sqrt();
    let mut h1dummy1 = h1hivalue1 / delta1;
    out.first[0] = h1dummy1;
    let h1relval = (h1dummy1 * reltol).abs();

    let mut temp = rclsqr / (4. * hilimit1);
    let mut temp2 = if temp >= 100.0 {
        0.0
    } else {
        erfc(temp.sqrt())
    };
    let mut temp3 = (-temp).exp();
    let temp4 = rclsqr.sqrt();
    let temp5 = cbyr.sqrt();

    let h2value = |hilimit: Real, temp2: Real, temp3: Real| {
        if hilimit != 0.0 {
            (hilimit + rclsqr * 0.5) * temp2 - (hilimit * rclsqr / PI).sqrt() * temp3
        } else {
            0.0
        }
    };
    let h3value = |hilimit: Real, temp2: Real, temp3: Real| {
        if hilimit != 0.0 {
            temp5 * (2. * (hilimit / PI).sqrt() * temp3 - temp4 * temp2)
        } else {
            0.0
        }
    };

    let mut h2hivalue1 = h2value(hilimit1, temp2, temp3);
    let mut h2dummy1 = h2hivalue1 / delta1;
    out.first[1] = h2dummy1;
    let h2relval = (h2dummy1 * reltol).abs();

    let mut h3hivalue1 = h3value(hilimit1, temp2, temp3);
    let mut h3dummy1 = h3hivalue1 / delta1;
    out.first[2] = h3dummy1;
    let h3relval = (h3dummy1 * reltol).abs();

    for i in (1..=index).rev() {
        delta1 = times[i] - times[i - 1];
        hilimit1 = curtime - times[i - 1];

        if doh1 {
            let h1hivalue2 = h1hivalue1;
            let h1dummy2 = h1dummy1;
            let h1lovalue1 = h1hivalue2;
            h1hivalue1 = (4. * cbyr * hilimit1 / PI).sqrt();
            h1dummy1 = (h1hivalue1 - h1lovalue1) / delta1;
            out.h1dash[i] = h1dummy1 - h1dummy2;
            if out.h1dash[i].abs() < h1relval {
                doh1 = false;
            }
        }
        if doh2 || doh3 {
            temp = rclsqr / (4. * hilimit1);
            temp2 = if temp >= 100.0 {
                0.0
            } else {
                erfc(temp.sqrt())
            };
            temp3 = (-temp).exp();
        }
        if doh2 {
            let h2hivalue2 = h2hivalue1;
            let h2dummy2 = h2dummy1;
            let h2lovalue1 = h2hivalue2;
            h2hivalue1 = h2value(hilimit1, temp2, temp3);
            h2dummy1 = (h2hivalue1 - h2lovalue1) / delta1;
            out.h2[i] = h2dummy1 - h2dummy2;
            if out.h2[i].abs() < h2relval {
                doh2 = false;
            }
        }
        if doh3 {
            let h3hivalue2 = h3hivalue1;
            let h3dummy2 = h3dummy1;
            let h3lovalue1 = h3hivalue2;
            h3hivalue1 = h3value(hilimit1, temp2, temp3);
            h3dummy1 = (h3hivalue1 - h3lovalue1) / delta1;
            out.h3dash[i] = h3dummy1 - h3dummy2;
            if out.h3dash[i].abs() < h3relval {
                doh3 = false;
            }
        }
    }
    out.aux_index = index;
    out
}

/// `LTRArlcCoeffsSetup` (the `G = 0` RLC line): coefficients at `curtime`
/// over the accepted times `times[0..=index]`, with delay `t`, `alpha`,
/// `beta` and C's chopping tolerance `reltol` (always 0 from a deck).
///
/// C quirk kept: at the auxiliary index the `h2` correction term integrates
/// the previous window `[lolimit2, hilimit2]` with the values at `TD` and at
/// the auxiliary time (C's commented-out `lolimit2 = T` fix is not applied).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn rlc_coefficients(
    t: Real,
    alpha: Real,
    beta: Real,
    curtime: Real,
    times: &[Real],
    reltol: Real,
) -> Coefficients {
    let index = times.len() - 1;
    let mut out = Coefficients::with_len(times.len());
    let (mut doh1, mut doh2, mut doh3) = (true, true, true);

    let aux = if t == 0.0 {
        index
    } else if curtime - t <= 0.0 {
        0
    } else {
        // The first sample (newest first) at least T back; one earlier when
        // exactly T back. curtime > T and times[0] = 0, so it exists, and an
        // exact hit cannot be sample 0.
        let mut found = 0;
        let mut exact = false;
        for i in (0..=index).rev() {
            if curtime - times[i] == t {
                exact = true;
                found = i;
                break;
            }
            if curtime - times[i] > t {
                found = i;
                break;
            }
        }
        if exact {
            found.saturating_sub(1)
        } else {
            found
        }
    };

    let mut h2lovalue1 = 0.0;
    let mut h2hivalue1 = 0.0;
    let mut h2dummy1 = 0.0;
    let mut h3hivalue1 = 0.0;
    let mut h3dummy1 = 0.0;
    let mut alphasq_t = 0.0;
    let mut exp_beta_t = 0.0;
    let mut h2relval = 0.0;
    let mut h3relval = 0.0;
    let mut besselarg;

    if aux != 0 {
        let lolimit1 = t;
        let hilimit1 = curtime - times[aux];
        let delta1 = hilimit1 - lolimit1;

        h2lovalue1 = rlc_h2(t, t, alpha, beta);
        besselarg = if hilimit1 > t {
            alpha * (hilimit1 * hilimit1 - t * t).sqrt()
        } else {
            0.0
        };
        let expterm = (-beta * hilimit1).exp();
        let bessi1overxterm = bess_i1_x_over_x(besselarg);
        alphasq_t = alpha * alpha * t;
        h2hivalue1 = if alpha == 0.0 || hilimit1 < t {
            0.0
        } else {
            alphasq_t * expterm * bessi1overxterm
        };
        h2dummy1 = twiceintlinfunc(
            lolimit1, hilimit1, lolimit1, h2lovalue1, h2hivalue1, lolimit1, hilimit1,
        ) / delta1;
        out.first[1] = h2dummy1;
        h2relval = (reltol * h2dummy1).abs();

        let h3lovalue1 = 0.0; // E3dash is consistent with this (C).
        let bessi0term = bess_i0(besselarg);
        exp_beta_t = (-beta * t).exp();
        h3hivalue1 = if hilimit1 <= t || beta == 0.0 {
            0.0
        } else {
            expterm * bessi0term - exp_beta_t
        };
        h3dummy1 = intlinfunc(
            lolimit1, hilimit1, h3lovalue1, h3hivalue1, lolimit1, hilimit1,
        ) / delta1;
        out.first[2] = h3dummy1;
        h3relval = (h3dummy1 * reltol).abs();
    }

    let h1value = |hilimit: Real, exparg: Real, expterm: Real| {
        if beta == 0.0 {
            hilimit
        } else if hilimit == 0.0 {
            0.0
        } else {
            (bess_i1(-exparg) + bess_i0(-exparg)) * hilimit * expterm - hilimit
        }
    };

    let mut lolimit1 = 0.0;
    let mut hilimit1 = curtime - times[index];
    let mut delta1 = hilimit1 - lolimit1;
    let mut exparg = -beta * hilimit1;
    let mut expterm = exparg.exp();

    let mut h1lovalue1;
    let mut h1hivalue1 = h1value(hilimit1, exparg, expterm);
    let mut h1dummy1 = h1hivalue1 / delta1;
    out.first[0] = h1dummy1;
    let h1relval = (h1dummy1 * reltol).abs();

    let mut lolimit2;
    let mut hilimit2 = 0.0;
    lolimit2 = 0.0;

    for i in (1..=index).rev() {
        if doh1 || doh2 || doh3 {
            lolimit2 = lolimit1;
            hilimit2 = hilimit1;
            lolimit1 = hilimit2;
            hilimit1 = curtime - times[i - 1];
            delta1 = times[i] - times[i - 1];
            exparg = -beta * hilimit1;
            expterm = exparg.exp();
        }
        if doh1 {
            let h1hivalue2 = h1hivalue1;
            let h1dummy2 = h1dummy1;
            h1lovalue1 = h1hivalue2;
            h1hivalue1 = h1value(hilimit1, exparg, expterm);
            h1dummy1 = (h1hivalue1 - h1lovalue1) / delta1;
            out.h1dash[i] = h1dummy1 - h1dummy2;
            if out.h1dash[i].abs() <= h1relval {
                doh1 = false;
            }
        }
        if i <= aux {
            besselarg = 0.0;
            if doh2 || doh3 {
                besselarg = if hilimit1 > t {
                    alpha * (hilimit1 * hilimit1 - t * t).sqrt()
                } else {
                    0.0
                };
            }
            if doh2 {
                let h2lovalue2 = h2lovalue1;
                let h2hivalue2 = h2hivalue1;
                let h2dummy2 = h2dummy1;
                h2lovalue1 = h2hivalue2;
                let bessi1overxterm = bess_i1_x_over_x(besselarg);
                h2hivalue1 = if alpha == 0.0 || hilimit1 < t {
                    0.0
                } else {
                    alphasq_t * expterm * bessi1overxterm
                };
                h2dummy1 = twiceintlinfunc(
                    lolimit1, hilimit1, lolimit1, h2lovalue1, h2hivalue1, lolimit1, hilimit1,
                ) / delta1;
                out.h2[i] = h2dummy1 - h2dummy2
                    + intlinfunc(
                        lolimit2, hilimit2, h2lovalue2, h2hivalue2, lolimit2, hilimit2,
                    );
                if out.h2[i].abs() <= h2relval {
                    doh2 = false;
                }
            }
            if doh3 {
                let h3hivalue2 = h3hivalue1;
                let h3dummy2 = h3dummy1;
                let h3lovalue1 = h3hivalue2;
                let bessi0term = bess_i0(besselarg);
                h3hivalue1 = if hilimit1 <= t || beta == 0.0 {
                    0.0
                } else {
                    expterm * bessi0term - exp_beta_t
                };
                h3dummy1 = intlinfunc(
                    lolimit1, hilimit1, h3lovalue1, h3hivalue1, lolimit1, hilimit1,
                ) / delta1;
                out.h3dash[i] = h3dummy1 - h3dummy2;
                if out.h3dash[i].abs() <= h3relval {
                    doh3 = false;
                }
            }
        }
    }
    out.aux_index = aux;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erfc_matches_reference_values() {
        // Reference values from an arbitrary-precision evaluation.
        for (x, expected) in [
            (0.0, 1.0),
            (0.1, 0.887_537_083_981_715_2),
            (0.5, 0.479_500_122_186_953_5),
            (1.0, 0.157_299_207_050_285_16),
            (1.9, 0.007_209_570_764_742_534),
            (2.0, 0.004_677_734_981_047_264_5),
            (3.0, 2.209_049_699_858_543_8e-5),
            (5.0, 1.537_459_794_428_035e-12),
            (9.5, 3.769_214_485_654_88e-41),
            (-1.0, 1.842_700_792_949_714_8),
        ] {
            let got = erfc(x);
            assert!(
                ((got - expected) / expected).abs() < 1e-12,
                "erfc({x}) = {got:e}, expected {expected:e}"
            );
        }
    }

    #[test]
    fn bessel_approximations_match_known_values() {
        // I0(1) = 1.2660658777520082, I1(1) = 0.5651591039924851,
        // I0(5) = 27.239871823604442, I1(5) = 24.335642142450524.
        // The A&S polynomials are accurate to about 1e-7 relative.
        for (got, expected) in [
            (bess_i0(1.), 1.266_065_877_752_008_2),
            (bess_i1(1.), 0.565_159_103_992_485_1),
            (bess_i0(5.), 27.239_871_823_604_442),
            (bess_i1(5.), 24.335_642_142_450_524),
            (bess_i1_x_over_x(5.), 24.335_642_142_450_524 / 5.),
            (bess_i1(-1.), -0.565_159_103_992_485_1),
        ] {
            assert!(
                ((got - expected) / expected).abs() < 2e-7,
                "{got} {expected}"
            );
        }
    }

    #[test]
    fn interpolation_reproduces_polynomials() {
        let [c1, c2, c3] = quad_interp(1.5, 0., 1., 3.).unwrap();
        let f = |t: f64| 2. * t * t - t + 1.;
        assert!((c1 * f(0.) + c2 * f(1.) + c3 * f(3.) - f(1.5)).abs() < 1e-12);
        assert_eq!(quad_interp(1., 0., 1., 3.), Some([0., 1., 0.]));
        assert_eq!(quad_interp(2., 0., 1., 1.), None);
        let [l1, l2] = lin_interp(0.25, 0., 1.).unwrap();
        assert_eq!((l1, l2), (0.75, 0.25));
        assert_eq!(lin_interp(1., 1., 1.), None);
    }

    #[test]
    fn rc_coefficients_sum_to_the_step_response() {
        // For a constant waveform the convolution of h1' collapses to its
        // integral: the first coefficient plus the history coefficients
        // telescope to (H(t) - H(t - t0)) / dt differences; check the
        // telescoping identity sum_i c_i = (first window) sum directly.
        let times = [0., 1e-9, 2e-9, 3e-9];
        let c = rc_coefficients(1e-12 / 100., 100. * 1e-12, 4e-9, &times, 0.);
        assert_eq!(c.h1dash.len(), 4);
        assert!(c.first.iter().all(|v| v.is_finite()));
        assert!(c.h1dash[1..].iter().all(|v| v.is_finite()));
        // Second differences of a concave sqrt are negative.
        assert!(c.h1dash[1..].iter().all(|v| *v < 0.));
    }

    #[test]
    fn max_safe_step_is_inside_the_bracket() {
        let td = 5e-9;
        let alpha = 0.5 * 10. / 250e-9;
        let step = rlc_max_safe_step(td, alpha, alpha, 1e-3, 1e-12);
        assert!(step > 0. && step <= 9. * td, "{step}");
    }
}
