//! C's `Dderivs` derivative arithmetic (`src/include/ngspice/distodef.h`,
//! `src/maths/deriv/*.c`), transcribed expression by expression so that the
//! distortion Taylor coefficients built from it (`bjtdset.c`) round as C's
//! do. Each value carries a function of three variables `p`, `q`, `r` and
//! its partial derivatives up to the third order (derivatives, not Taylor
//! coefficients: `d2_p2` is the second derivative).

// The statement-by-statement transcription (fields assigned in C's order,
// later ones reading earlier ones) is deliberate.
#![allow(
    clippy::suspicious_operation_groupings,
    clippy::many_single_char_names,
    clippy::field_reassign_with_default
)]

use crate::primitives::Real;

/// C's `Dderivs`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[allow(missing_docs)]
pub(crate) struct Dderivs {
    pub value: Real,
    pub d1_p: Real,
    pub d1_q: Real,
    pub d1_r: Real,
    pub d2_p2: Real,
    pub d2_q2: Real,
    pub d2_r2: Real,
    pub d2_pq: Real,
    pub d2_qr: Real,
    pub d2_pr: Real,
    pub d3_p3: Real,
    pub d3_q3: Real,
    pub d3_r3: Real,
    pub d3_p2r: Real,
    pub d3_p2q: Real,
    pub d3_q2r: Real,
    pub d3_pq2: Real,
    pub d3_pr2: Real,
    pub d3_qr2: Real,
    pub d3_pqr: Real,
}

impl Dderivs {
    /// The variable `p`, `q` or `r` (`index` 0, 1, 2) at zero: bjtdset.c's
    /// `d_p`, `d_q`, `d_r`.
    pub(crate) fn variable(index: usize) -> Self {
        let mut d = Self::default();
        match index {
            0 => d.d1_p = 1.0,
            1 => d.d1_q = 1.0,
            _ => d.d1_r = 1.0,
        }
        d
    }

    /// C `plusder.c`.
    pub(crate) fn plus(&self, other: &Self) -> Self {
        Self {
            value: self.value + other.value,
            d1_p: self.d1_p + other.d1_p,
            d1_q: self.d1_q + other.d1_q,
            d1_r: self.d1_r + other.d1_r,
            d2_p2: self.d2_p2 + other.d2_p2,
            d2_q2: self.d2_q2 + other.d2_q2,
            d2_r2: self.d2_r2 + other.d2_r2,
            d2_pq: self.d2_pq + other.d2_pq,
            d2_qr: self.d2_qr + other.d2_qr,
            d2_pr: self.d2_pr + other.d2_pr,
            d3_p3: self.d3_p3 + other.d3_p3,
            d3_q3: self.d3_q3 + other.d3_q3,
            d3_r3: self.d3_r3 + other.d3_r3,
            d3_p2r: self.d3_p2r + other.d3_p2r,
            d3_p2q: self.d3_p2q + other.d3_p2q,
            d3_q2r: self.d3_q2r + other.d3_q2r,
            d3_pq2: self.d3_pq2 + other.d3_pq2,
            d3_pr2: self.d3_pr2 + other.d3_pr2,
            d3_qr2: self.d3_qr2 + other.d3_qr2,
            d3_pqr: self.d3_pqr + other.d3_pqr,
        }
    }

    /// C `timesder.c` (`k * old`).
    pub(crate) fn times(&self, k: Real) -> Self {
        Self {
            value: k * self.value,
            d1_p: k * self.d1_p,
            d1_q: k * self.d1_q,
            d1_r: k * self.d1_r,
            d2_p2: k * self.d2_p2,
            d2_q2: k * self.d2_q2,
            d2_r2: k * self.d2_r2,
            d2_pq: k * self.d2_pq,
            d2_qr: k * self.d2_qr,
            d2_pr: k * self.d2_pr,
            d3_p3: k * self.d3_p3,
            d3_q3: k * self.d3_q3,
            d3_r3: k * self.d3_r3,
            d3_p2r: k * self.d3_p2r,
            d3_p2q: k * self.d3_p2q,
            d3_q2r: k * self.d3_q2r,
            d3_pq2: k * self.d3_pq2,
            d3_pr2: k * self.d3_pr2,
            d3_qr2: k * self.d3_qr2,
            d3_pqr: k * self.d3_pqr,
        }
    }

    /// C `expderiv.c`.
    pub(crate) fn exp(&self) -> Self {
        let t = *self;
        let mut n = Self::default();
        n.value = t.value.exp();
        n.d1_p = n.value * t.d1_p;
        n.d1_q = n.value * t.d1_q;
        n.d1_r = n.value * t.d1_r;
        n.d2_p2 = n.value * t.d2_p2 + t.d1_p * n.d1_p;
        n.d2_q2 = n.value * t.d2_q2 + t.d1_q * n.d1_q;
        n.d2_r2 = n.value * t.d2_r2 + t.d1_r * n.d1_r;
        n.d2_pq = n.value * t.d2_pq + t.d1_p * n.d1_q;
        n.d2_qr = n.value * t.d2_qr + t.d1_q * n.d1_r;
        n.d2_pr = n.value * t.d2_pr + t.d1_p * n.d1_r;
        n.d3_p3 = n.value * t.d3_p3 + t.d2_p2 * n.d1_p + t.d2_p2 * n.d1_p + n.d2_p2 * t.d1_p;
        n.d3_q3 = n.value * t.d3_q3 + t.d2_q2 * n.d1_q + t.d2_q2 * n.d1_q + n.d2_q2 * t.d1_q;
        n.d3_r3 = n.value * t.d3_r3 + t.d2_r2 * n.d1_r + t.d2_r2 * n.d1_r + n.d2_r2 * t.d1_r;
        n.d3_p2r = n.value * t.d3_p2r + t.d2_p2 * n.d1_r + t.d2_pr * n.d1_p + n.d2_pr * t.d1_p;
        n.d3_p2q = n.value * t.d3_p2q + t.d2_p2 * n.d1_q + t.d2_pq * n.d1_p + n.d2_pq * t.d1_p;
        n.d3_q2r = n.value * t.d3_q2r + t.d2_q2 * n.d1_r + t.d2_qr * n.d1_q + n.d2_qr * t.d1_q;
        n.d3_pq2 = n.value * t.d3_pq2 + t.d2_q2 * n.d1_p + t.d2_pq * n.d1_q + n.d2_pq * t.d1_q;
        n.d3_pr2 = n.value * t.d3_pr2 + t.d2_r2 * n.d1_p + t.d2_pr * n.d1_r + n.d2_pr * t.d1_r;
        n.d3_qr2 = n.value * t.d3_qr2 + t.d2_r2 * n.d1_q + t.d2_qr * n.d1_r + n.d2_qr * t.d1_r;
        n.d3_pqr = n.value * t.d3_pqr + t.d2_pq * n.d1_r + t.d2_pr * n.d1_q + n.d2_qr * t.d1_p;
        n
    }

    /// C `invderiv.c`.
    pub(crate) fn inv(&self) -> Self {
        let t = *self;
        let mut n = Self::default();
        n.value = 1.0 / t.value;
        n.d1_p = -n.value * n.value * t.d1_p;
        n.d1_q = -n.value * n.value * t.d1_q;
        n.d1_r = -n.value * n.value * t.d1_r;
        n.d2_p2 = -n.value * (2.0 * n.d1_p * t.d1_p + n.value * t.d2_p2);
        n.d2_q2 = -n.value * (2.0 * n.d1_q * t.d1_q + n.value * t.d2_q2);
        n.d2_r2 = -n.value * (2.0 * n.d1_r * t.d1_r + n.value * t.d2_r2);
        n.d2_pq = -n.value * (2.0 * n.d1_q * t.d1_p + n.value * t.d2_pq);
        n.d2_qr = -n.value * (2.0 * n.d1_r * t.d1_q + n.value * t.d2_qr);
        n.d2_pr = -n.value * (2.0 * n.d1_r * t.d1_p + n.value * t.d2_pr);
        n.d3_p3 = -(2.0
            * (t.d1_p * n.d1_p * n.d1_p
                + n.value * (n.d2_p2 * t.d1_p + n.d1_p * t.d2_p2 + n.d1_p * t.d2_p2))
            + n.value * n.value * t.d3_p3);
        n.d3_q3 = -(2.0
            * (t.d1_q * n.d1_q * n.d1_q
                + n.value * (n.d2_q2 * t.d1_q + n.d1_q * t.d2_q2 + n.d1_q * t.d2_q2))
            + n.value * n.value * t.d3_q3);
        n.d3_r3 = -(2.0
            * (t.d1_r * n.d1_r * n.d1_r
                + n.value * (n.d2_r2 * t.d1_r + n.d1_r * t.d2_r2 + n.d1_r * t.d2_r2))
            + n.value * n.value * t.d3_r3);
        n.d3_p2r = -(2.0
            * (t.d1_p * n.d1_p * n.d1_r
                + n.value * (n.d2_pr * t.d1_p + n.d1_p * t.d2_pr + n.d1_r * t.d2_p2))
            + n.value * n.value * t.d3_p2r);
        n.d3_p2q = -(2.0
            * (t.d1_p * n.d1_p * n.d1_q
                + n.value * (n.d2_pq * t.d1_p + n.d1_p * t.d2_pq + n.d1_q * t.d2_p2))
            + n.value * n.value * t.d3_p2q);
        n.d3_q2r = -(2.0
            * (t.d1_q * n.d1_q * n.d1_r
                + n.value * (n.d2_qr * t.d1_q + n.d1_q * t.d2_qr + n.d1_r * t.d2_q2))
            + n.value * n.value * t.d3_q2r);
        n.d3_pq2 = -(2.0
            * (t.d1_q * n.d1_q * n.d1_p
                + n.value * (n.d2_pq * t.d1_q + n.d1_q * t.d2_pq + n.d1_p * t.d2_q2))
            + n.value * n.value * t.d3_pq2);
        n.d3_pr2 = -(2.0
            * (t.d1_r * n.d1_r * n.d1_p
                + n.value * (n.d2_pr * t.d1_r + n.d1_r * t.d2_pr + n.d1_p * t.d2_r2))
            + n.value * n.value * t.d3_pr2);
        n.d3_qr2 = -(2.0
            * (t.d1_r * n.d1_r * n.d1_q
                + n.value * (n.d2_qr * t.d1_r + n.d1_r * t.d2_qr + n.d1_q * t.d2_r2))
            + n.value * n.value * t.d3_qr2);
        n.d3_pqr = -(2.0
            * (t.d1_p * n.d1_q * n.d1_r
                + n.value * (n.d2_qr * t.d1_p + n.d1_q * t.d2_pr + n.d1_r * t.d2_pq))
            + n.value * n.value * t.d3_pqr);
        n
    }

    /// C `multder.c`.
    pub(crate) fn mul(&self, other: &Self) -> Self {
        let (a, b) = (*self, *other);
        let mut n = Self::default();
        n.value = a.value * b.value;
        n.d1_p = a.d1_p * b.value + a.value * b.d1_p;
        n.d1_q = a.d1_q * b.value + a.value * b.d1_q;
        n.d1_r = a.d1_r * b.value + a.value * b.d1_r;
        n.d2_p2 = a.d2_p2 * b.value + a.d1_p * b.d1_p + a.d1_p * b.d1_p + a.value * b.d2_p2;
        n.d2_q2 = a.d2_q2 * b.value + a.d1_q * b.d1_q + a.d1_q * b.d1_q + a.value * b.d2_q2;
        n.d2_r2 = a.d2_r2 * b.value + a.d1_r * b.d1_r + a.d1_r * b.d1_r + a.value * b.d2_r2;
        n.d2_pq = a.d2_pq * b.value + a.d1_p * b.d1_q + a.d1_q * b.d1_p + a.value * b.d2_pq;
        n.d2_qr = a.d2_qr * b.value + a.d1_q * b.d1_r + a.d1_r * b.d1_q + a.value * b.d2_qr;
        n.d2_pr = a.d2_pr * b.value + a.d1_p * b.d1_r + a.d1_r * b.d1_p + a.value * b.d2_pr;
        n.d3_p3 = a.d3_p3 * b.value
            + a.d2_p2 * b.d1_p
            + a.d2_p2 * b.d1_p
            + b.d2_p2 * a.d1_p
            + b.d2_p2 * a.d1_p
            + a.d2_p2 * b.d1_p
            + b.d2_p2 * a.d1_p
            + a.value * b.d3_p3;
        n.d3_q3 = a.d3_q3 * b.value
            + a.d2_q2 * b.d1_q
            + a.d2_q2 * b.d1_q
            + b.d2_q2 * a.d1_q
            + b.d2_q2 * a.d1_q
            + a.d2_q2 * b.d1_q
            + b.d2_q2 * a.d1_q
            + a.value * b.d3_q3;
        n.d3_r3 = a.d3_r3 * b.value
            + a.d2_r2 * b.d1_r
            + a.d2_r2 * b.d1_r
            + b.d2_r2 * a.d1_r
            + b.d2_r2 * a.d1_r
            + a.d2_r2 * b.d1_r
            + b.d2_r2 * a.d1_r
            + a.value * b.d3_r3;
        n.d3_p2r = a.d3_p2r * b.value
            + a.d2_p2 * b.d1_r
            + a.d2_pr * b.d1_p
            + b.d2_p2 * a.d1_r
            + b.d2_pr * a.d1_p
            + a.d2_pr * b.d1_p
            + b.d2_pr * a.d1_p
            + a.value * b.d3_p2r;
        n.d3_p2q = a.d3_p2q * b.value
            + a.d2_p2 * b.d1_q
            + a.d2_pq * b.d1_p
            + b.d2_p2 * a.d1_q
            + b.d2_pq * a.d1_p
            + a.d2_pq * b.d1_p
            + b.d2_pq * a.d1_p
            + a.value * b.d3_p2q;
        n.d3_q2r = a.d3_q2r * b.value
            + a.d2_q2 * b.d1_r
            + a.d2_qr * b.d1_q
            + b.d2_q2 * a.d1_r
            + b.d2_qr * a.d1_q
            + a.d2_qr * b.d1_q
            + b.d2_qr * a.d1_q
            + a.value * b.d3_q2r;
        n.d3_pq2 = a.d3_pq2 * b.value
            + a.d2_q2 * b.d1_p
            + a.d2_pq * b.d1_q
            + b.d2_q2 * a.d1_p
            + b.d2_pq * a.d1_q
            + a.d2_pq * b.d1_q
            + b.d2_pq * a.d1_q
            + a.value * b.d3_pq2;
        n.d3_pr2 = a.d3_pr2 * b.value
            + a.d2_r2 * b.d1_p
            + a.d2_pr * b.d1_r
            + b.d2_r2 * a.d1_p
            + b.d2_pr * a.d1_r
            + a.d2_pr * b.d1_r
            + b.d2_pr * a.d1_r
            + a.value * b.d3_pr2;
        n.d3_qr2 = a.d3_qr2 * b.value
            + a.d2_r2 * b.d1_q
            + a.d2_qr * b.d1_r
            + b.d2_r2 * a.d1_q
            + b.d2_qr * a.d1_r
            + a.d2_qr * b.d1_r
            + b.d2_qr * a.d1_r
            + a.value * b.d3_qr2;
        n.d3_pqr = a.d3_pqr * b.value
            + a.d2_pq * b.d1_r
            + a.d2_pr * b.d1_q
            + b.d2_pq * a.d1_r
            + b.d2_qr * a.d1_p
            + a.d2_qr * b.d1_p
            + b.d2_pr * a.d1_q
            + a.value * b.d3_pqr;
        n
    }

    /// C `divderiv.c`.
    pub(crate) fn div(&self, other: &Self) -> Self {
        let (a, b) = (*self, *other);
        let mut n = Self::default();
        n.value = a.value / b.value;
        n.d1_p = (a.d1_p - a.value * b.d1_p / b.value) / b.value;
        n.d1_q = (a.d1_q - a.value * b.d1_q / b.value) / b.value;
        n.d1_r = (a.d1_r - a.value * b.d1_r / b.value) / b.value;
        n.d2_p2 = (a.d2_p2 - b.d1_p * n.d1_p - n.value * b.d2_p2
            + (b.d1_p * (n.value * b.d1_p - a.d1_p) / b.value))
            / b.value;
        n.d2_q2 = (a.d2_q2 - b.d1_q * n.d1_q - n.value * b.d2_q2
            + (b.d1_q * (n.value * b.d1_q - a.d1_q) / b.value))
            / b.value;
        n.d2_r2 = (a.d2_r2 - b.d1_r * n.d1_r - n.value * b.d2_r2
            + b.d1_r * (n.value * b.d1_r - a.d1_r) / b.value)
            / b.value;
        n.d2_pq = (a.d2_pq - b.d1_q * n.d1_p - n.value * b.d2_pq
            + b.d1_p * (n.value * b.d1_q - a.d1_q) / b.value)
            / b.value;
        n.d2_qr = (a.d2_qr - b.d1_r * n.d1_q - n.value * b.d2_qr
            + b.d1_q * (n.value * b.d1_r - a.d1_r) / b.value)
            / b.value;
        n.d2_pr = (a.d2_pr - b.d1_r * n.d1_p - n.value * b.d2_pr
            + b.d1_p * (n.value * b.d1_r - a.d1_r) / b.value)
            / b.value;
        n.d3_p3 = (-b.d1_p * n.d2_p2 + a.d3_p3
            - b.d2_p2 * n.d1_p
            - b.d1_p * n.d2_p2
            - n.d1_p * b.d2_p2
            - n.value * b.d3_p3
            + (b.d1_p * (n.d1_p * b.d1_p + n.value * b.d2_p2 - a.d2_p2)
                + (n.value * b.d1_p - a.d1_p) * (b.d2_p2 - b.d1_p * b.d1_p / b.value))
                / b.value)
            / b.value;
        n.d3_q3 = (-b.d1_q * n.d2_q2 + a.d3_q3
            - b.d2_q2 * n.d1_q
            - b.d1_q * n.d2_q2
            - n.d1_q * b.d2_q2
            - n.value * b.d3_q3
            + (b.d1_q * (n.d1_q * b.d1_q + n.value * b.d2_q2 - a.d2_q2)
                + (n.value * b.d1_q - a.d1_q) * (b.d2_q2 - b.d1_q * b.d1_q / b.value))
                / b.value)
            / b.value;
        n.d3_r3 = (-b.d1_r * n.d2_r2 + a.d3_r3
            - b.d2_r2 * n.d1_r
            - b.d1_r * n.d2_r2
            - n.d1_r * b.d2_r2
            - n.value * b.d3_r3
            + (b.d1_r * (n.d1_r * b.d1_r + n.value * b.d2_r2 - a.d2_r2)
                + (n.value * b.d1_r - a.d1_r) * (b.d2_r2 - b.d1_r * b.d1_r / b.value))
                / b.value)
            / b.value;
        n.d3_p2r = (-b.d1_r * n.d2_p2 + a.d3_p2r
            - b.d2_pr * n.d1_p
            - b.d1_p * n.d2_pr
            - n.d1_r * b.d2_p2
            - n.value * b.d3_p2r
            + (b.d1_p * (n.d1_r * b.d1_p + n.value * b.d2_pr - a.d2_pr)
                + (n.value * b.d1_p - a.d1_p) * (b.d2_pr - b.d1_p * b.d1_r / b.value))
                / b.value)
            / b.value;
        n.d3_p2q = (-b.d1_q * n.d2_p2 + a.d3_p2q
            - b.d2_pq * n.d1_p
            - b.d1_p * n.d2_pq
            - n.d1_q * b.d2_p2
            - n.value * b.d3_p2q
            + (b.d1_p * (n.d1_q * b.d1_p + n.value * b.d2_pq - a.d2_pq)
                + (n.value * b.d1_p - a.d1_p) * (b.d2_pq - b.d1_p * b.d1_q / b.value))
                / b.value)
            / b.value;
        n.d3_q2r = (-b.d1_r * n.d2_q2 + a.d3_q2r
            - b.d2_qr * n.d1_q
            - b.d1_q * n.d2_qr
            - n.d1_r * b.d2_q2
            - n.value * b.d3_q2r
            + (b.d1_q * (n.d1_r * b.d1_q + n.value * b.d2_qr - a.d2_qr)
                + (n.value * b.d1_q - a.d1_q) * (b.d2_qr - b.d1_q * b.d1_r / b.value))
                / b.value)
            / b.value;
        n.d3_pq2 = (-b.d1_p * n.d2_q2 + a.d3_pq2
            - b.d2_pq * n.d1_q
            - b.d1_q * n.d2_pq
            - n.d1_p * b.d2_q2
            - n.value * b.d3_pq2
            + (b.d1_q * (n.d1_p * b.d1_q + n.value * b.d2_pq - a.d2_pq)
                + (n.value * b.d1_q - a.d1_q) * (b.d2_pq - b.d1_q * b.d1_p / b.value))
                / b.value)
            / b.value;
        n.d3_pr2 = (-b.d1_p * n.d2_r2 + a.d3_pr2
            - b.d2_pr * n.d1_r
            - b.d1_r * n.d2_pr
            - n.d1_p * b.d2_r2
            - n.value * b.d3_pr2
            + (b.d1_r * (n.d1_p * b.d1_r + n.value * b.d2_pr - a.d2_pr)
                + (n.value * b.d1_r - a.d1_r) * (b.d2_pr - b.d1_r * b.d1_p / b.value))
                / b.value)
            / b.value;
        n.d3_qr2 = (-b.d1_q * n.d2_r2 + a.d3_qr2
            - b.d2_qr * n.d1_r
            - b.d1_r * n.d2_qr
            - n.d1_q * b.d2_r2
            - n.value * b.d3_qr2
            + (b.d1_r * (n.d1_q * b.d1_r + n.value * b.d2_qr - a.d2_qr)
                + (n.value * b.d1_r - a.d1_r) * (b.d2_qr - b.d1_r * b.d1_q / b.value))
                / b.value)
            / b.value;
        n.d3_pqr = (-b.d1_r * n.d2_pq + a.d3_pqr
            - b.d2_qr * n.d1_p
            - b.d1_q * n.d2_pr
            - n.d1_r * b.d2_pq
            - n.value * b.d3_pqr
            + (b.d1_p * (n.d1_r * b.d1_q + n.value * b.d2_qr - a.d2_qr)
                + (n.value * b.d1_q - a.d1_q) * (b.d2_pr - b.d1_p * b.d1_r / b.value))
                / b.value)
            / b.value;
        n
    }

    /// C `sqrtder.c`.
    pub(crate) fn sqrt(&self) -> Self {
        let t = *self;
        let mut n = Self::default();
        n.value = t.value.sqrt();
        if t.value == 0.0 {
            return n;
        }
        n.d1_p = 0.5 * t.d1_p / n.value;
        n.d1_q = 0.5 * t.d1_q / n.value;
        n.d1_r = 0.5 * t.d1_r / n.value;
        n.d2_p2 = 0.5 / n.value * (t.d2_p2 - 0.5 * t.d1_p * t.d1_p / t.value);
        n.d2_q2 = 0.5 / n.value * (t.d2_q2 - 0.5 * t.d1_q * t.d1_q / t.value);
        n.d2_r2 = 0.5 / n.value * (t.d2_r2 - 0.5 * t.d1_r * t.d1_r / t.value);
        n.d2_pq = 0.5 / n.value * (t.d2_pq - 0.5 * t.d1_p * t.d1_q / t.value);
        n.d2_qr = 0.5 / n.value * (t.d2_qr - 0.5 * t.d1_q * t.d1_r / t.value);
        n.d2_pr = 0.5 / n.value * (t.d2_pr - 0.5 * t.d1_p * t.d1_r / t.value);
        n.d3_p3 = 0.5
            * (t.d3_p3 / n.value
                - 0.5 / (t.value * n.value)
                    * (-1.5 / t.value * t.d1_p * t.d1_p * t.d1_p
                        + t.d1_p * t.d2_p2
                        + t.d1_p * t.d2_p2
                        + t.d1_p * t.d2_p2));
        n.d3_q3 = 0.5
            * (t.d3_q3 / n.value
                - 0.5 / (t.value * n.value)
                    * (-1.5 / t.value * t.d1_q * t.d1_q * t.d1_q
                        + t.d1_q * t.d2_q2
                        + t.d1_q * t.d2_q2
                        + t.d1_q * t.d2_q2));
        n.d3_r3 = 0.5
            * (t.d3_r3 / n.value
                - 0.5 / (t.value * n.value)
                    * (-1.5 / t.value * t.d1_r * t.d1_r * t.d1_r
                        + t.d1_r * t.d2_r2
                        + t.d1_r * t.d2_r2
                        + t.d1_r * t.d2_r2));
        n.d3_p2r = 0.5
            * (t.d3_p2r / n.value
                - 0.5 / (t.value * n.value)
                    * (-1.5 / t.value * t.d1_p * t.d1_p * t.d1_r
                        + t.d1_p * t.d2_pr
                        + t.d1_p * t.d2_pr
                        + t.d1_r * t.d2_p2));
        n.d3_p2q = 0.5
            * (t.d3_p2q / n.value
                - 0.5 / (t.value * n.value)
                    * (-1.5 / t.value * t.d1_p * t.d1_p * t.d1_q
                        + t.d1_p * t.d2_pq
                        + t.d1_p * t.d2_pq
                        + t.d1_q * t.d2_p2));
        n.d3_q2r = 0.5
            * (t.d3_q2r / n.value
                - 0.5 / (t.value * n.value)
                    * (-1.5 / t.value * t.d1_q * t.d1_q * t.d1_r
                        + t.d1_q * t.d2_qr
                        + t.d1_q * t.d2_qr
                        + t.d1_r * t.d2_q2));
        n.d3_pq2 = 0.5
            * (t.d3_pq2 / n.value
                - 0.5 / (t.value * n.value)
                    * (-1.5 / t.value * t.d1_q * t.d1_q * t.d1_p
                        + t.d1_q * t.d2_pq
                        + t.d1_q * t.d2_pq
                        + t.d1_p * t.d2_q2));
        n.d3_pr2 = 0.5
            * (t.d3_pr2 / n.value
                - 0.5 / (t.value * n.value)
                    * (-1.5 / t.value * t.d1_r * t.d1_r * t.d1_p
                        + t.d1_r * t.d2_pr
                        + t.d1_r * t.d2_pr
                        + t.d1_p * t.d2_r2));
        n.d3_qr2 = 0.5
            * (t.d3_qr2 / n.value
                - 0.5 / (t.value * n.value)
                    * (-1.5 / t.value * t.d1_r * t.d1_r * t.d1_q
                        + t.d1_r * t.d2_qr
                        + t.d1_r * t.d2_qr
                        + t.d1_q * t.d2_r2));
        n.d3_pqr = 0.5
            * (t.d3_pqr / n.value
                - 0.5 / (t.value * n.value)
                    * (-1.5 / t.value * t.d1_p * t.d1_q * t.d1_r
                        + t.d1_p * t.d2_qr
                        + t.d1_q * t.d2_pr
                        + t.d1_r * t.d2_pq));
        n
    }

    /// C `tanderiv.c`.
    pub(crate) fn tan(&self) -> Self {
        let t = *self;
        let mut n = Self::default();
        n.value = t.value.tan();
        n.d1_p = (1.0 + n.value * n.value) * t.d1_p;
        n.d1_q = (1.0 + n.value * n.value) * t.d1_q;
        n.d1_r = (1.0 + n.value * n.value) * t.d1_r;
        n.d2_p2 = (1.0 + n.value * n.value) * t.d2_p2 + 2.0 * n.value * t.d1_p * n.d1_p;
        n.d2_q2 = (1.0 + n.value * n.value) * t.d2_q2 + 2.0 * n.value * t.d1_q * n.d1_q;
        n.d2_r2 = (1.0 + n.value * n.value) * t.d2_r2 + 2.0 * n.value * t.d1_r * n.d1_r;
        n.d2_pq = (1.0 + n.value * n.value) * t.d2_pq + 2.0 * n.value * t.d1_p * n.d1_q;
        n.d2_qr = (1.0 + n.value * n.value) * t.d2_qr + 2.0 * n.value * t.d1_q * n.d1_r;
        n.d2_pr = (1.0 + n.value * n.value) * t.d2_pr + 2.0 * n.value * t.d1_p * n.d1_r;
        n.d3_p3 = (1.0 + n.value * n.value) * t.d3_p3
            + 2.0
                * (n.value * (t.d2_p2 * n.d1_p + t.d2_p2 * n.d1_p + n.d2_p2 * t.d1_p)
                    + t.d1_p * n.d1_p * n.d1_p);
        n.d3_q3 = (1.0 + n.value * n.value) * t.d3_q3
            + 2.0
                * (n.value * (t.d2_q2 * n.d1_q + t.d2_q2 * n.d1_q + n.d2_q2 * t.d1_q)
                    + t.d1_q * n.d1_q * n.d1_q);
        n.d3_r3 = (1.0 + n.value * n.value) * t.d3_r3
            + 2.0
                * (n.value * (t.d2_r2 * n.d1_r + t.d2_r2 * n.d1_r + n.d2_r2 * t.d1_r)
                    + t.d1_r * n.d1_r * n.d1_r);
        n.d3_p2r = (1.0 + n.value * n.value) * t.d3_p2r
            + 2.0
                * (n.value * (t.d2_p2 * n.d1_r + t.d2_pr * n.d1_p + n.d2_pr * t.d1_p)
                    + t.d1_p * n.d1_p * n.d1_r);
        n.d3_p2q = (1.0 + n.value * n.value) * t.d3_p2q
            + 2.0
                * (n.value * (t.d2_p2 * n.d1_q + t.d2_pq * n.d1_p + n.d2_pq * t.d1_p)
                    + t.d1_p * n.d1_p * n.d1_q);
        n.d3_q2r = (1.0 + n.value * n.value) * t.d3_q2r
            + 2.0
                * (n.value * (t.d2_q2 * n.d1_r + t.d2_qr * n.d1_q + n.d2_qr * t.d1_q)
                    + t.d1_q * n.d1_q * n.d1_r);
        n.d3_pq2 = (1.0 + n.value * n.value) * t.d3_pq2
            + 2.0
                * (n.value * (t.d2_q2 * n.d1_p + t.d2_pq * n.d1_q + n.d2_pq * t.d1_q)
                    + t.d1_q * n.d1_q * n.d1_p);
        n.d3_pr2 = (1.0 + n.value * n.value) * t.d3_pr2
            + 2.0
                * (n.value * (t.d2_r2 * n.d1_p + t.d2_pr * n.d1_r + n.d2_pr * t.d1_r)
                    + t.d1_r * n.d1_r * n.d1_p);
        n.d3_qr2 = (1.0 + n.value * n.value) * t.d3_qr2
            + 2.0
                * (n.value * (t.d2_r2 * n.d1_q + t.d2_qr * n.d1_r + n.d2_qr * t.d1_r)
                    + t.d1_r * n.d1_r * n.d1_q);
        n.d3_pqr = (1.0 + n.value * n.value) * t.d3_pqr
            + 2.0
                * (n.value * (t.d2_pq * n.d1_r + t.d2_pr * n.d1_q + n.d2_qr * t.d1_p)
                    + t.d1_p * n.d1_q * n.d1_r);
        n
    }
}
