//! Device nonlinearities for small-signal `.disto` distortion analysis.
//!
//! C reference: `src/spicelib/analysis/distoan.c` drives a Volterra-series
//! analysis whose device hooks (`DEVdisto`: `dio/diodisto.c`,
//! `bjt/bjtdisto.c`, `mos1/mos1dist.c`) do two things:
//!
//! * `D_SETUP` (`diodset.c`, `bjtdset.c`, `mos1dset.c`) stores, at the
//!   operating point, the second- and third-order Taylor coefficients of every
//!   nonlinear branch current or charge with respect to up to three
//!   controlling voltages;
//! * `D_TWOF1`, `D_THRF1`, `D_F1PF2`, `D_F1MF2`, `D_2F1MF2` evaluate those
//!   polynomials on the lower-order Volterra kernels (the node solutions at the
//!   lower-order frequencies) through `dloadfns.c` and add the result, times
//!   `j omega` for a charge, to the right-hand side of the next solve.
//!
//! The port keeps that split. A device describes its nonlinearities **at the
//! bias point** through [`crate::devices::Device::distortion`], returning a
//! [`DeviceDistortion`]: a list of [`DistortionTerm`]s, each a Taylor
//! polynomial ([`Taylor`]) of a current or charge flowing between two nodes in
//! up to three [`Control`] voltages. The analysis owns the kernels, the
//! frequencies and the polynomial evaluation ([`add_products`], C
//! `dloadfns.c`). Linear devices, which C distorts only through their AC
//! matrix, return [`DeviceDistortion::Linear`]; independent sources with
//! `distof1`/`distof2` inputs return [`DeviceDistortion::Input`]. The trait
//! default is an explicit `NotYetPorted` error, so a device whose distortion
//! is not ported can never be treated as linear by accident.
//!
//! [`Series3`] is the truncated three-variable Taylor arithmetic standing in
//! for C's `Dderivs` (`src/maths/deriv/*.c`), which `bjtdset.c` uses to
//! differentiate the Gummel-Poon expressions.

use crate::devices::{Circuit, MnaUnknowns, ModelContext};
use crate::maths::Vector;
use crate::primitives::{Complex, NodeId, Real, SpiceError, SpiceResult};

/// A controlling voltage: the sum of the node differences `v(a) - v(b)` of
/// its pairs. Almost always one pair; C occasionally forms sums (the BJT's
/// `vbx` kernel is `(vb - vb') + (vb' - vc')`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    /// The `[a, b]` node pairs whose differences are summed.
    pub pairs: Vec<[NodeId; 2]>,
    /// Further pairs, summed like [`Self::pairs`] except in the `f1 - f2`
    /// product, where C reads their `H1(f2)` kernel **without** the
    /// conjugation that makes it `H1(-f2)` (`bjtdisto.c`'s `D_F1MF2` base-
    /// resistance kernel `i1hm2z` lacks the minus sign). Empty except for
    /// that BJT quirk.
    pub unconjugated_in_f1_minus_f2: Vec<[NodeId; 2]>,
}

impl Control {
    /// The voltage `v(a) - v(b)`.
    #[must_use]
    pub fn between(a: NodeId, b: NodeId) -> Self {
        Self::sum(vec![[a, b]])
    }

    /// The sum of the differences of `pairs`.
    #[must_use]
    pub fn sum(pairs: Vec<[NodeId; 2]>) -> Self {
        Self {
            pairs,
            unconjugated_in_f1_minus_f2: Vec::new(),
        }
    }

    /// Adds `pairs` as [`Self::unconjugated_in_f1_minus_f2`] pairs.
    #[must_use]
    pub fn with_unconjugated_in_f1_minus_f2(mut self, pairs: Vec<[NodeId; 2]>) -> Self {
        self.unconjugated_in_f1_minus_f2.extend(pairs);
        self
    }

    fn sum_of(pairs: &[[NodeId; 2]], unknowns: &MnaUnknowns, x: &[Complex]) -> Complex {
        let at = |node: NodeId| {
            unknowns
                .node_row(node)
                .and_then(|row| x.get(row).copied())
                .unwrap_or(Complex::ZERO)
        };
        pairs
            .iter()
            .fold(Complex::ZERO, |sum, [a, b]| sum + (at(*a) - at(*b)))
    }

    /// The control's value in the node solution `x` (ground is zero).
    #[must_use]
    pub fn value(&self, unknowns: &MnaUnknowns, x: &[Complex]) -> Complex {
        Self::sum_of(&self.pairs, unknowns, x)
            + Self::sum_of(&self.unconjugated_in_f1_minus_f2, unknowns, x)
    }

    /// The control's `H1(-f2)` value from the `H1(f2)` solution `x` in the
    /// `f1 - f2` product: conjugated, except the
    /// [`Self::unconjugated_in_f1_minus_f2`] pairs.
    fn minus_f2_value(&self, unknowns: &MnaUnknowns, x: &[Complex]) -> Complex {
        Self::sum_of(&self.pairs, unknowns, x).conj()
            + Self::sum_of(&self.unconjugated_in_f1_minus_f2, unknowns, x)
    }
}

/// Second- and third-order Taylor coefficients of a function of the controls
/// `x`, `y`, `z` (C's `cxx`, ..., `cxyz` arguments of `dloadfns.c`): the
/// function's expansion is `xx x^2 + yy y^2 + ... + xy x y + ... + xxx x^3 +
/// ... + xyz x y z` beyond its linear part. Missing controls have zero
/// coefficients.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[allow(missing_docs)]
pub struct Taylor {
    pub xx: Real,
    pub yy: Real,
    pub zz: Real,
    pub xy: Real,
    pub yz: Real,
    pub xz: Real,
    pub xxx: Real,
    pub yyy: Real,
    pub zzz: Real,
    pub xxy: Real,
    pub xxz: Real,
    pub xyy: Real,
    pub yyz: Real,
    pub xzz: Real,
    pub yzz: Real,
    pub xyz: Real,
}

impl Taylor {
    /// A one-variable polynomial `c2 x^2 + c3 x^3` (C's `D1*` functions).
    #[must_use]
    pub fn single(c2: Real, c3: Real) -> Self {
        Self {
            xx: c2,
            xxx: c3,
            ..Self::default()
        }
    }

    fn second(&self) -> [(usize, usize, Real); 6] {
        [
            (0, 0, self.xx),
            (1, 1, self.yy),
            (2, 2, self.zz),
            (0, 1, self.xy),
            (1, 2, self.yz),
            (0, 2, self.xz),
        ]
    }

    fn third(&self) -> [(usize, usize, usize, Real); 10] {
        [
            (0, 0, 0, self.xxx),
            (1, 1, 1, self.yyy),
            (2, 2, 2, self.zzz),
            (0, 0, 1, self.xxy),
            (0, 0, 2, self.xxz),
            (0, 1, 1, self.xyy),
            (1, 1, 2, self.yyz),
            (0, 2, 2, self.xzz),
            (1, 2, 2, self.yzz),
            (0, 1, 2, self.xyz),
        ]
    }

    fn is_finite(&self) -> bool {
        self.second().iter().all(|c| c.2.is_finite())
            && self.third().iter().all(|c| c.3.is_finite())
    }
}

/// Whether a term's polynomial is a current or a charge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Response {
    /// A memoryless current.
    Current,
    /// A charge: its current is `j omega` times the polynomial, `omega` being
    /// the angular frequency of the response being solved for.
    Charge,
}

/// One nonlinearity: the higher-order part of a current or charge flowing
/// from `nodes[0]` to `nodes[1]` (C subtracts the evaluated polynomial from
/// the right-hand side at `nodes[0]` and adds it at `nodes[1]`).
#[derive(Debug, Clone, PartialEq)]
pub struct DistortionTerm {
    /// Current or charge.
    pub response: Response,
    /// The branch the response flows through.
    pub nodes: [NodeId; 2],
    /// The controls `x`, `y`, `z` (one to three).
    pub controls: Vec<Control>,
    /// The Taylor coefficients in those controls.
    pub taylor: Taylor,
    /// C quirk: `mos1dist.c`'s `D_2F1MF2` case reads its `H2(f1, f1)`
    /// kernel from the first-order `H1(f1)` vector (`r1H1ptr` where
    /// `r2H11ptr` is meant). Set by [`Self::with_first_order_im3_kernel`] for
    /// MOS1 terms so that `2f1 - f2` reproduces C.
    pub first_order_im3_kernel: bool,
}

impl DistortionTerm {
    /// A term.
    #[must_use]
    pub fn new(
        response: Response,
        nodes: [NodeId; 2],
        controls: Vec<Control>,
        taylor: Taylor,
    ) -> Self {
        Self {
            response,
            nodes,
            controls,
            taylor,
            first_order_im3_kernel: false,
        }
    }

    /// Marks the term with `mos1dist.c`'s `2f1 - f2` kernel quirk (see
    /// [`Self::first_order_im3_kernel`]).
    #[must_use]
    pub fn with_first_order_im3_kernel(mut self) -> Self {
        self.first_order_im3_kernel = true;
        self
    }
}

/// A distortion input of an independent source (`distof1`/`distof2`,
/// `vsrcpar.c`/`isrcpar.c`): magnitude and phase in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DistortionInput {
    /// Magnitude (C defaults it to 1 when the setter has no value).
    pub magnitude: Real,
    /// Phase in degrees (default 0).
    pub phase: Real,
}

impl DistortionInput {
    /// The half-amplitude phasor C drives the first-order solve with
    /// (`cktdisto.c`: `0.5 * mag * (cos, sin)(pi * phase / 180)`).
    #[must_use]
    pub fn phasor(self) -> Complex {
        let angle = std::f64::consts::PI * self.phase / 180.;
        Complex::new(
            0.5 * self.magnitude * angle.cos(),
            0.5 * self.magnitude * angle.sin(),
        )
    }
}

/// What a device contributes to `.disto`.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceDistortion {
    /// Linear: the device enters only through its small-signal matrix (C
    /// devices without `DEVdisto`).
    Linear,
    /// An independent source with distortion inputs (it is otherwise linear).
    Input {
        /// The `distof1` input, if given.
        f1: Option<DistortionInput>,
        /// The `distof2` input, if given.
        f2: Option<DistortionInput>,
    },
    /// Nonlinear terms at the bias point (possibly none).
    Terms(Vec<DistortionTerm>),
}

/// What [`crate::devices::Device::distortion`] sees: the converged operating
/// point (C's `CKTrhsOld` at `D_SETUP`).
#[derive(Debug, Clone, Copy)]
pub struct DistortionContext<'a> {
    /// Circuit/nominal temperature and junction `gmin`.
    pub model_context: &'a ModelContext,
    /// Node numbering, with ground eliminated.
    pub unknowns: &'a MnaUnknowns,
    /// The operating-point solution.
    pub bias: &'a Vector,
}

impl DistortionContext<'_> {
    /// The operating-point voltage of `node`, zero at ground.
    #[must_use]
    pub fn voltage(&self, node: NodeId) -> Real {
        self.unknowns
            .node_row(node)
            .and_then(|row| self.bias.get(row))
            .unwrap_or(0.)
    }
}

/// Every device's distortion description at the bias: the nonlinear terms of
/// the whole circuit and the sources' inputs, by source name.
#[derive(Debug, Clone, Default)]
pub struct CircuitDistortion {
    /// All nonlinear terms.
    pub terms: Vec<DistortionTerm>,
    /// `(source name, distof1, distof2)` of the sources with an input.
    pub inputs: Vec<(String, Option<DistortionInput>, Option<DistortionInput>)>,
}

/// Collects [`CircuitDistortion`] at the operating point `bias`.
///
/// # Errors
/// Stale numbering, a bias of the wrong size, a device whose distortion is
/// not ported (the [`crate::devices::Device::distortion`] default), invalid
/// device physics at the bias, or a nonfinite coefficient.
pub fn circuit_distortion(
    circuit: &Circuit,
    context: &ModelContext,
    bias: &Vector,
) -> SpiceResult<CircuitDistortion> {
    if bias.len() != circuit.unknown_count() || !bias.is_finite() {
        return Err(SpiceError::circuit(
            "invalid distortion bias dimensions/values",
        ));
    }
    let mut result = CircuitDistortion::default();
    let distortion_context = DistortionContext {
        model_context: context,
        unknowns: circuit.unknowns(),
        bias,
    };
    for device in circuit.devices() {
        match device.distortion(&distortion_context)? {
            DeviceDistortion::Linear => {}
            DeviceDistortion::Input { f1, f2 } => {
                let finite = |input: Option<DistortionInput>| {
                    input.is_none_or(|i| i.magnitude.is_finite() && i.phase.is_finite())
                };
                if !finite(f1) || !finite(f2) {
                    return Err(SpiceError::circuit(format!(
                        "{}: nonfinite distortion input",
                        device.name()
                    )));
                }
                if f1.is_some() || f2.is_some() {
                    result.inputs.push((device.name().to_owned(), f1, f2));
                }
            }
            DeviceDistortion::Terms(terms) => {
                for term in terms {
                    if term.controls.is_empty() || term.controls.len() > 3 {
                        return Err(SpiceError::circuit(format!(
                            "{}: a distortion term needs one to three controls",
                            device.name()
                        )));
                    }
                    if !term.taylor.is_finite() {
                        return Err(SpiceError::Numerical {
                            context: format!("distortion of {}", device.name()),
                            message: "nonfinite Taylor coefficient".into(),
                        });
                    }
                    result.terms.push(term);
                }
            }
        }
    }
    Ok(result)
}

/// The higher-order response being solved for (C's `D_TWOF1` ... modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Product {
    /// `2 f1`: second-order kernel `H2(f1, f1)`.
    TwoF1,
    /// `3 f1`: third-order kernel `H3(f1, f1, f1)`.
    ThreeF1,
    /// `f1 + f2`: `H2(f1, f2)`.
    F1PlusF2,
    /// `f1 - f2`: `H2(f1, -f2)`.
    F1MinusF2,
    /// `2 f1 - f2`: `H3(f1, f1, -f2)`.
    TwoF1MinusF2,
}

/// The lower-order kernels a [`Product`] is built from: node solutions of
/// the earlier solves at the same `f1`.
#[derive(Debug, Clone, Copy)]
pub struct Kernels<'a> {
    /// `H1(f1)`.
    pub h1: &'a [Complex],
    /// `H1(f2)` (the IM products; conjugated for `-f2` by [`add_products`]).
    pub h1_f2: Option<&'a [Complex]>,
    /// `H2(f1, f1)` (`3 f1` and `2 f1 - f2`).
    pub h2: Option<&'a [Complex]>,
    /// `H2(f1, -f2)` (`2 f1 - f2`).
    pub h2_minus: Option<&'a [Complex]>,
}

fn scaled(c: Complex, k: Real) -> Complex {
    Complex::new(c.re * k, c.im * k)
}

/// Adds every term's contribution to `rhs` for `product` (C `DEVdisto` with
/// the `dloadfns.c` polynomial evaluators): with `X`, `B`, `X2`, `M` the
/// control values in `H1(f1)`, `H1(+-f2)`, `H2(f1,f1)` and `H2(f1,-f2)`,
///
/// | product | value of `c_ij v_i v_j` / `c_ijk v_i v_j v_k` |
/// | --- | --- |
/// | `2f1` | `X_i X_j` / 0 |
/// | `3f1` | `X_i X2_j + X_j X2_i` / `X_i X_j X_k` |
/// | `f1 +- f2` | `(X_i B_j + X_j B_i) / 2` / 0 |
/// | `2f1 - f2` | `(2 (X_i M_j + X_j M_i) + B_i X2_j + B_j X2_i) / 3` / `(X_i X_j B_k + X_i X_k B_j + X_j X_k B_i) / 3` |
///
/// A charge's value is multiplied by `j omega`, `omega` being the response's
/// angular frequency (negative for `f1 - f2` below `f2`, as in C).
///
/// # Errors
/// A kernel the product needs is missing, or `rhs` has the wrong size.
pub fn add_products(
    terms: &[DistortionTerm],
    product: Product,
    kernels: Kernels<'_>,
    omega: Real,
    unknowns: &MnaUnknowns,
    rhs: &mut [Complex],
) -> SpiceResult<()> {
    let missing = || SpiceError::circuit("distortion kernel missing for this product");
    if rhs.len() != kernels.h1.len() {
        return Err(SpiceError::circuit("distortion right-hand side size"));
    }
    let need = |kernel: Option<&[Complex]>| -> SpiceResult<Option<()>> {
        match kernel {
            Some(k) if k.len() == rhs.len() => Ok(Some(())),
            Some(_) => Err(SpiceError::circuit("distortion kernel size")),
            None => Ok(None),
        }
    };
    need(kernels.h1_f2)?;
    need(kernels.h2)?;
    need(kernels.h2_minus)?;
    for term in terms {
        let values = |kernel: &[Complex]| -> [Complex; 3] {
            let mut v = [Complex::ZERO; 3];
            for (slot, control) in v.iter_mut().zip(&term.controls) {
                *slot = control.value(unknowns, kernel);
            }
            v
        };
        let x = values(kernels.h1);
        let second = term.taylor.second();
        let third = term.taylor.third();
        let mut sum = Complex::ZERO;
        match product {
            Product::TwoF1 => {
                for (i, j, c) in second {
                    sum = sum + scaled(x[i] * x[j], c);
                }
            }
            Product::ThreeF1 => {
                let x2 = values(kernels.h2.ok_or_else(missing)?);
                for (i, j, c) in second {
                    sum = sum + scaled(x[i] * x2[j] + x[j] * x2[i], c);
                }
                for (i, j, k, c) in third {
                    sum = sum + scaled(x[i] * x[j] * x[k], c);
                }
            }
            Product::F1PlusF2 | Product::F1MinusF2 => {
                let h1_f2 = kernels.h1_f2.ok_or_else(missing)?;
                let mut b = values(h1_f2);
                if product == Product::F1MinusF2 {
                    for (slot, control) in b.iter_mut().zip(&term.controls) {
                        *slot = control.minus_f2_value(unknowns, h1_f2);
                    }
                }
                for (i, j, c) in second {
                    sum = sum + scaled(x[i] * b[j] + x[j] * b[i], 0.5 * c);
                }
            }
            Product::TwoF1MinusF2 => {
                let b = values(kernels.h1_f2.ok_or_else(missing)?).map(Complex::conj);
                let x2 = if term.first_order_im3_kernel {
                    x
                } else {
                    values(kernels.h2.ok_or_else(missing)?)
                };
                let m = values(kernels.h2_minus.ok_or_else(missing)?);
                let mut inner = Complex::ZERO;
                for (i, j, c) in second {
                    let mixed = x[i] * m[j] + x[j] * m[i];
                    inner = inner + scaled(scaled(mixed, 2.) + b[i] * x2[j] + b[j] * x2[i], c);
                }
                for (i, j, k, c) in third {
                    inner = inner
                        + scaled(
                            x[i] * x[j] * b[k] + x[i] * x[k] * b[j] + x[k] * x[j] * b[i],
                            c,
                        );
                }
                sum = scaled(inner, 1. / 3.);
            }
        }
        if term.response == Response::Charge {
            sum = Complex::new(-omega * sum.im, omega * sum.re);
        }
        let [a, b] = term.nodes;
        if let Some(row) = unknowns.node_row(a) {
            rhs[row] = rhs[row] - sum;
        }
        if let Some(row) = unknowns.node_row(b) {
            rhs[row] = rhs[row] + sum;
        }
    }
    Ok(())
}

/// The monomials `p^a q^b r^c` with `a + b + c <= 3`, in [`Series3`] order.
const MONOMIALS: [[u8; 3]; 20] = [
    [0, 0, 0],
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
    [2, 0, 0],
    [0, 2, 0],
    [0, 0, 2],
    [1, 1, 0],
    [0, 1, 1],
    [1, 0, 1],
    [3, 0, 0],
    [0, 3, 0],
    [0, 0, 3],
    [2, 1, 0],
    [2, 0, 1],
    [1, 2, 0],
    [0, 2, 1],
    [1, 0, 2],
    [0, 1, 2],
    [1, 1, 1],
];

fn monomial(exponents: [u8; 3]) -> Option<usize> {
    MONOMIALS.iter().position(|m| *m == exponents)
}

/// A function of three variables `p`, `q`, `r` as its Taylor polynomial at a
/// point, truncated after the third order: C's `Dderivs`
/// (`src/include/ngspice/distodef.h`, `src/maths/deriv/*.c`) in coefficient
/// rather than derivative form (`d2_p2 = 2 c_pp`, `d2_pq = c_pq`,
/// `d3_p3 = 6 c_ppp`, `d3_p2q = 2 c_ppq`, `d3_pqr = c_pqr`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Series3 {
    c: [Real; 20],
}

impl Series3 {
    /// The constant `value`.
    #[must_use]
    pub fn constant(value: Real) -> Self {
        let mut c = [0.; 20];
        c[0] = value;
        Self { c }
    }

    /// The variable `p`, `q` or `r` (`index` 0, 1, 2) at `value`.
    #[must_use]
    pub fn variable(index: usize, value: Real) -> Self {
        let mut s = Self::constant(value);
        s.c[1 + index.min(2)] = 1.;
        s
    }

    /// A function of one variable (`index`) from its value and derivatives
    /// `d1`, `d2`, `d3` (C's pattern of setting `value`, `d1_p`, `d2_p2`,
    /// `d3_p3` directly).
    #[must_use]
    pub fn univariate(index: usize, value: Real, d1: Real, d2: Real, d3: Real) -> Self {
        let i = index.min(2);
        let mut s = Self::constant(value);
        let mut e = [0u8; 3];
        e[i] = 1;
        s.c[monomial(e).unwrap_or(0)] = d1;
        e[i] = 2;
        s.c[monomial(e).unwrap_or(0)] = d2 / 2.;
        e[i] = 3;
        s.c[monomial(e).unwrap_or(0)] = d3 / 6.;
        s
    }

    /// The value at the expansion point.
    #[must_use]
    pub const fn value(&self) -> Real {
        self.c[0]
    }

    /// The Taylor coefficient of `p^a q^b r^c` (zero beyond third order).
    #[must_use]
    pub fn coefficient(&self, exponents: [u8; 3]) -> Real {
        monomial(exponents).map_or(0., |i| self.c[i])
    }

    /// Sets the Taylor coefficient of `p^a q^b r^c` (ignored beyond third
    /// order).
    pub fn set_coefficient(&mut self, exponents: [u8; 3], value: Real) {
        if let Some(i) = monomial(exponents) {
            self.c[i] = value;
        }
    }

    /// `self + other` (C `PlusDeriv`).
    #[must_use]
    pub fn plus(&self, other: &Self) -> Self {
        let mut c = self.c;
        for (a, b) in c.iter_mut().zip(other.c) {
            *a += b;
        }
        Self { c }
    }

    /// `k * self` (C `TimesDeriv`).
    #[must_use]
    pub fn times(&self, k: Real) -> Self {
        Self {
            c: self.c.map(|v| k * v),
        }
    }

    /// `self + k` (C's `value += k`).
    #[must_use]
    pub fn offset(&self, k: Real) -> Self {
        let mut s = *self;
        s.c[0] += k;
        s
    }

    /// `self * other`, truncated (C `MultDeriv`).
    #[must_use]
    pub fn mul(&self, other: &Self) -> Self {
        let mut c = [0.; 20];
        for (i, a) in MONOMIALS.iter().enumerate() {
            if self.c[i] == 0. {
                continue;
            }
            for (j, b) in MONOMIALS.iter().enumerate() {
                let e = [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
                if let Some(k) = monomial(e) {
                    c[k] += self.c[i] * other.c[j];
                }
            }
        }
        Self { c }
    }

    /// `f(self)` for `f` given by its value and first three derivatives at
    /// `self.value()`.
    fn compose(&self, f: [Real; 4]) -> Self {
        let mut delta = *self;
        delta.c[0] = 0.;
        let delta2 = delta.mul(&delta);
        let delta3 = delta2.mul(&delta);
        Self::constant(f[0])
            .plus(&delta.times(f[1]))
            .plus(&delta2.times(f[2] / 2.))
            .plus(&delta3.times(f[3] / 6.))
    }

    /// `1 / self` (C `InvDeriv`).
    #[must_use]
    pub fn inv(&self) -> Self {
        let v = self.value();
        let r = 1. / v;
        self.compose([r, -r * r, 2. * r * r * r, -6. * r * r * r * r])
    }

    /// `self / other` (C `DivDeriv`).
    #[must_use]
    pub fn div(&self, other: &Self) -> Self {
        self.mul(&other.inv())
    }

    /// `sqrt(self)` (C `SqrtDeriv`, whose derivatives are all zero at a zero
    /// argument).
    #[must_use]
    pub fn sqrt(&self) -> Self {
        let v = self.value();
        let s = v.sqrt();
        if v == 0. {
            return Self::constant(s);
        }
        self.compose([s, 0.5 / s, -0.25 / (v * s), 0.375 / (v * v * s)])
    }

    /// `exp(self)` (C `ExpDeriv`).
    #[must_use]
    pub fn exp(&self) -> Self {
        let e = self.value().exp();
        self.compose([e, e, e, e])
    }

    /// `tan(self)` (C `TanDeriv`).
    #[must_use]
    pub fn tan(&self) -> Self {
        let t = self.value().tan();
        let s = 1. + t * t;
        self.compose([t, s, 2. * t * s, 2. * s * (1. + 3. * t * t)])
    }
}

#[cfg(test)]
mod tests {
    use super::{Series3, Taylor};

    /// A function and its exact Taylor coefficients at a point.
    fn check(s: &Series3, f: impl Fn(f64, f64, f64) -> f64, at: [f64; 3]) {
        // Central finite differences of the closed form.
        let h = 1e-3;
        let value = f(at[0], at[1], at[2]);
        assert!((s.value() - value).abs() <= 1e-12 * value.abs().max(1.));
        let d = |e: [i32; 3]| {
            let point = |dp: f64, dq: f64, dr: f64| f(at[0] + dp, at[1] + dq, at[2] + dr);
            match e {
                [1, 0, 0] => (point(h, 0., 0.) - point(-h, 0., 0.)) / (2. * h),
                [0, 1, 0] => (point(0., h, 0.) - point(0., -h, 0.)) / (2. * h),
                [2, 0, 0] => (point(h, 0., 0.) - 2. * value + point(-h, 0., 0.)) / (h * h) / 2.,
                [1, 1, 0] => {
                    (point(h, h, 0.) - point(h, -h, 0.) - point(-h, h, 0.) + point(-h, -h, 0.))
                        / (4. * h * h)
                }
                [3, 0, 0] => {
                    (point(2. * h, 0., 0.) - 2. * point(h, 0., 0.) + 2. * point(-h, 0., 0.)
                        - point(-2. * h, 0., 0.))
                        / (2. * h * h * h)
                        / 6.
                }
                _ => unreachable!(),
            }
        };
        for e in [[1, 0, 0], [0, 1, 0], [2, 0, 0], [1, 1, 0], [3, 0, 0]] {
            let want = d(e);
            let got = s.coefficient([e[0] as u8, e[1] as u8, e[2] as u8]);
            assert!(
                (got - want).abs() <= 1e-4 * want.abs().max(1e-3),
                "{e:?}: {got} vs {want}"
            );
        }
    }

    #[test]
    fn series_arithmetic_matches_closed_forms() {
        let at = [0.3, 0.7, -0.2];
        let p = Series3::variable(0, at[0]);
        let q = Series3::variable(1, at[1]);
        let r = Series3::variable(2, at[2]);
        let f = p.mul(&q).plus(&r).exp().div(&q.offset(1.).sqrt());
        check(&f, |p, q, r| (p * q + r).exp() / (q + 1.).sqrt(), at);
        let g = p.times(2.).tan().mul(&p.inv());
        check(&g, |p, _, _| (2. * p).tan() / p, at);
        // The mixed third-order coefficient of p q r is exactly 1.
        let pqr = p.mul(&q).mul(&r);
        assert_eq!(pqr.coefficient([1, 1, 1]), 1.);
        assert_eq!(Series3::constant(0.).sqrt(), Series3::constant(0.));
        let t = Taylor::single(2., 3.);
        assert_eq!((t.xx, t.xxx, t.yy), (2., 3., 0.));
    }
}
