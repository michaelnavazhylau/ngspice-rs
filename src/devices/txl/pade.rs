//! TXL coefficient construction, following `txlsetup.c`'s `ReadTxL`,
//! `y_pade`, `exp_pade`, `get_h3` and `update_h1C_c`.
//! Local arrays replace C's file-global scratch variables. Setup failures
//! return contextual errors instead of terminating the process.
use crate::primitives::{Complex, Real, SpiceError, SpiceResult};

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Term {
    pub c: Real,
    pub x: Real,
}
#[derive(Debug, Clone)]
pub(super) struct Pade {
    pub admit: Real,
    /// Delay in picoseconds (C's `taul`).
    pub delay: Real,
    pub lossless: bool,
    pub complex: bool,
    pub h1: [Term; 3],
    pub h2: [Term; 3],
    pub h3: [Term; 6],
    pub h1c: Real,
    pub atten2: Real,
    pub atten3: Real,
}
fn error(message: &str) -> SpiceError {
    SpiceError::Numerical {
        context: "TXL txlsetup.c Padé coefficients".into(),
        message: message.into(),
    }
}
fn eval(a: Real, b: Real, c: Real, x: Real) -> Real {
    a * x * x + b * x + c
}

/// The two 3x4 Gauss-Jordan solves of `Gaussian_Elimination1/2`, including
/// their different absolute pivot limits and partial-row swap order.
///
/// The loops stay index-based because this is C's row/column algorithm: the
/// row swap uses the pivot row that partial pivoting just chose, and the
/// elimination reads the row it is updating.
#[allow(clippy::needless_range_loop)]
fn eliminate(mut a: [[Real; 4]; 3], threshold: Real) -> SpiceResult<[Real; 3]> {
    for i in 0..3 {
        let mut pivot = i;
        for j in i + 1..3 {
            if a[j][i].abs() > a[pivot][i].abs() {
                pivot = j;
            }
        }
        if !a[pivot][i].is_finite() || a[pivot][i].abs() < threshold {
            return Err(error(
                "Gaussian elimination cannot choose a pivot; check model parameters",
            ));
        }
        if pivot != i {
            for k in i..4 {
                let value = a[i][k];
                a[i][k] = a[pivot][k];
                a[pivot][k] = value;
            }
        }
        let f = 1. / a[i][i];
        a[i][i] = 1.;
        for value in &mut a[i][i + 1..] {
            *value *= f;
        }
        for j in 0..3 {
            if i == j {
                continue;
            }
            let f = a[j][i];
            a[j][i] = 0.;
            for k in i + 1..4 {
                a[j][k] -= f * a[i][k];
            }
        }
    }
    Ok([a[2][3], a[1][3], a[0][3]])
}
fn solve_series(b: [Real; 5], dc: Real, threshold: Real) -> SpiceResult<([Real; 3], [Real; 3])> {
    let [b1, b2, b3, b4, b5] = b;
    let p = eliminate(
        [[1. - dc, b1, b2, -b3], [b1, b2, b3, -b4], [b2, b3, b4, -b5]],
        threshold,
    )?;
    let [p1, p2, p3] = p;
    Ok((p, [p1 + b1, b1 * p1 + p2 + b2, p3 * dc]))
}
/// `find_roots`/`exp_find_roots`: Cardano seed, at most 31 Newton updates,
/// and stable quadratic deflation. For complex propagation roots the last
/// two entries hold the real and imaginary part of the conjugate pair.
fn roots([a, b, c]: [Real; 3], propagation: bool) -> SpiceResult<([Real; 3], bool)> {
    let q = (a * a - 3. * b) / 9.;
    let p = (2. * a * a * a - 9. * a * b + 27. * c) / 54.;
    let d = q * q * q - p * p;
    let mut x = if d >= 0. {
        -2. * q.sqrt() * ((p / (q * q.sqrt())).acos() / 3.).cos() - a / 3.
    } else if p > 0. {
        let t = ((-d).sqrt() + p).powf(1. / 3.);
        -(t + q / t) - a / 3.
    } else if p == 0. {
        -a / 3.
    } else {
        let t = ((-d).sqrt() - p).powf(1. / 3.);
        t + q / t - a / 3.
    };
    let backup = x;
    for i in 1..=32 {
        let t = x - (x * x * x + a * x * x + b * x + c) / (3. * x * x + 2. * a * x + b);
        if (t - x).abs() <= 5e-4 || !t.is_finite() {
            break;
        }
        if i == 32 {
            x = backup;
            break;
        }
        x = t;
    }
    let a = a + x;
    let b = -c / x;
    let d = a * a - 4. * b;
    let pair = d < 0.;
    let (y, z) = if pair {
        if !propagation {
            return Err(error(
                "two imaginary characteristic-admittance roots (C rejects this model)",
            ));
        }
        (-0.5 * a, 0.5 * (-d).sqrt())
    } else {
        let scale: Real = if propagation { 1e-16 } else { 1e-18 };
        let d = (d * scale).sqrt() / scale.sqrt();
        let y = if a >= 0. {
            -0.5 * (a + d)
        } else {
            -0.5 * (a - d)
        };
        (y, b / y)
    };
    if [x, y, z].iter().any(|x| !x.is_finite()) {
        return Err(error("nonfinite cubic roots"));
    }
    Ok(([x, y, z], pair))
}
fn residues(p: [Real; 3], q: [Real; 3], x: [Real; 3], pair: bool) -> [Real; 3] {
    let [p1, p2, p3] = p;
    let [a, b, c] = [q[0] - p1, q[1] - p2, q[2] - p3];
    let mut r = x.map(|x| eval(a, b, c, x) / eval(3., 2. * p1, p2, x));
    if pair {
        let z = Complex::new(x[1], x[2]);
        let num = Complex::real(a) * z * z + Complex::real(b) * z + Complex::real(c);
        let den = Complex::real(3.) * z * z + Complex::real(2. * p1) * z + Complex::real(p2);
        let v = num / den;
        r[1] = v.re;
        r[2] = v.im;
    }
    r
}
impl Pade {
    pub(super) fn new(r: Real, l: Real, g: Real, c: Real, length: Real) -> SpiceResult<Self> {
        let l = l.max(1e-12);
        let admit = (c / l).sqrt();
        let tau = (l * c).sqrt();
        let mut h = Self {
            admit,
            delay: tau * length * 1e12,
            lossless: r / l < 5e5 && g < 1e-2,
            complex: false,
            h1: [Term::default(); 3],
            h2: [Term::default(); 3],
            h3: [Term::default(); 6],
            h1c: 0.,
            atten2: 1.,
            atten3: admit,
        };
        if !h.admit.is_finite() || h.admit <= 0. || !h.delay.is_finite() || h.delay <= 0. {
            return Err(error("nonfinite or degenerate line admittance/delay"));
        }
        if h.lossless {
            return Ok(h);
        }
        // mac(): derivatives of sqrt((1+a z)/(1+b z)), before factorials.
        let a = g / c;
        let b = r / l;
        let delta = a - b;
        let t = 3. * b * b - 2. * a * b - a * a;
        let y1 = 0.5 * delta;
        let y2 = 0.5 * t * y1 / delta;
        let y3 = ((3. * b * b + a * a) * y1 * y1 + 0.5 * t * y2) / delta;
        let y4 = ((3. * b * b - 3. * a * a) * y1 * y1 * y1
            + (9. * b * b + 3. * a * a) * y1 * y2
            + 0.5 * t * y3)
            / delta;
        let y5 = (12. * a * a * y1 * y1 * y1 * y1
            + y1 * y1 * y2 * (18. * b * b - 18. * a * a)
            + (9. * b * b + 3. * a * a) * (y2 * y2 + y1 * y3)
            + (3. * b * b + a * a) * y1 * y3
            + 0.5 * t * y4)
            / delta;
        let (p, q) = solve_series(
            [y1, y2 / 2., y3 / 6., y4 / 24., y5 / 120.],
            (a / b).sqrt(),
            1e-16,
        )?;
        let (x, _) = roots(p, false)?;
        let cs = residues(p, q, x, false);
        for i in 0..3 {
            h.h1[i] = Term { c: cs[i], x: x[i] };
        }
        // exp_pade(): expansion of exp(-length*sqrt((sL+R)(sC+G))).
        let y1 = 0.5 * (a + b);
        let y2 = a * b - y1 * y1;
        let y3 = -3. * y1 * y2;
        let y4 = -3. * y2 * y2 - 4. * y1 * y3;
        let y5 = -5. * y1 * y4 - 10. * y2 * y3;
        let y6 = -10. * y3 * y3 - 15. * y2 * y4 - 6. * y1 * y5;
        let a0 = y1 * tau * length;
        let mut aa = [0.; 6];
        let mut power = tau * tau;
        for (i, (v, factor)) in [(y2, 2.), (y3, 6.), (y4, 24.), (y5, 120.), (y6, 720.)]
            .into_iter()
            .enumerate()
        {
            aa[i + 1] = -v * power / factor * length;
            power *= tau;
        }
        let mut bb = [0.; 6];
        bb[0] = 1.;
        bb[1] = aa[1];
        for i in 2..6 {
            for j in 1..=i {
                bb[i] += j as Real * aa[j] * bb[i - j];
            }
            bb[i] /= i as Real;
        }
        let dc = (a0 - length * (r * g).sqrt()).exp();
        let (mut p, mut q) = solve_series([bb[1], bb[2], bb[3], bb[4], bb[5]], dc, 1e-28)?;
        for (i, divisor) in [tau, tau * tau, tau * tau * tau].into_iter().enumerate() {
            p[i] /= divisor;
            q[i] /= divisor;
        }
        let (x, pair) = roots(p, true)?;
        h.complex = pair;
        let cs = residues(p, q, x, pair);
        for i in 0..3 {
            h.h2[i] = Term { c: cs[i], x: x[i] };
        }
        h.atten2 = (-a0).exp();
        h.atten3 = h.atten2 * admit;
        // get_h3(): residues of the product H1*H2, before attenuation.
        for i in 0..3 {
            let t = h.h1[i];
            let mut sum = 0.;
            if pair {
                let [u, v, w] = h.h2;
                sum += u.c / (t.x - u.x)
                    + 2. * (v.c * t.x - w.x * w.c - v.x * v.c)
                        / (t.x * t.x - 2. * v.x * t.x + v.x * v.x + w.x * w.x);
            } else {
                for u in h.h2 {
                    sum += u.c / (t.x - u.x);
                }
            }
            h.h3[i] = Term {
                x: t.x,
                c: t.c + t.c * sum,
            };
        }
        for i in 0..3 {
            let t = h.h2[i];
            let mut sum = 0.;
            for u in h.h1 {
                sum += u.c / (t.x - u.x);
            }
            h.h3[i + 3] = Term {
                x: t.x,
                c: t.c + t.c * sum,
            };
        }
        if pair {
            let c = Complex::new(h.h2[1].c, h.h2[2].c);
            let x = Complex::new(h.h2[1].x, h.h2[2].x);
            let mut res = c;
            for u in h.h1 {
                res = res + (c / (x - Complex::real(u.x))) * Complex::real(u.c);
            }
            h.h3[4].c = res.re;
            h.h3[5].c = res.im;
        }
        for t in &mut h.h1 {
            t.c *= admit;
            h.h1c += t.c;
        }
        for t in &mut h.h2 {
            t.c *= h.atten2;
        }
        for t in &mut h.h3 {
            t.c *= h.atten3;
        }
        if !h.delay.is_finite()
            || h.delay <= 0.
            || h.h1
                .iter()
                .chain(&h.h2)
                .chain(&h.h3)
                .any(|t| !t.c.is_finite() || !t.x.is_finite() || t.x == 0.)
            || !h.h1c.is_finite()
        {
            return Err(error("nonfinite or degenerate Padé coefficients"));
        }
        Ok(h)
    }
}
