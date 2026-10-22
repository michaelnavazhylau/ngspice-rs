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
//! `deriv::Dderivs` transcribes C's derivative arithmetic (`Dderivs`,
//! `src/maths/deriv/*.c`), which `bjtdset.c` uses to differentiate the
//! Gummel-Poon expressions, and the `dloadfns` module C's load functions.

use crate::devices::{Circuit, MnaUnknowns, ModelContext};
use crate::maths::Vector;
use crate::primitives::{Complex, NodeId, Real, SpiceError, SpiceResult};

pub(crate) mod deriv;
mod dloadfns;

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
    /// Pairs subtracted from the sum of [`Self::pairs`], as `mos1dist.c`
    /// forms `vbd` as `vbs - vds` from the kernels' differences (rather than
    /// `vb - vd`, which rounds differently). Empty for most controls.
    pub minus: Vec<[NodeId; 2]>,
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
            minus: Vec::new(),
        }
    }

    /// The difference `(v(a) - v(b)) - (v(c) - v(d))` of two node
    /// differences, evaluated in that order on the kernels.
    #[must_use]
    pub fn difference(plus: [NodeId; 2], minus: [NodeId; 2]) -> Self {
        Self {
            minus: vec![minus],
            ..Self::between(plus[0], plus[1])
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
        self.signed(unknowns, x) + Self::sum_of(&self.unconjugated_in_f1_minus_f2, unknowns, x)
    }

    /// The [`Self::pairs`] sum less the [`Self::minus`] sum.
    fn signed(&self, unknowns: &MnaUnknowns, x: &[Complex]) -> Complex {
        let sum = Self::sum_of(&self.pairs, unknowns, x);
        if self.minus.is_empty() {
            sum
        } else {
            sum - Self::sum_of(&self.minus, unknowns, x)
        }
    }

    /// The control's `H1(-f2)` value from the `H1(f2)` solution `x` in the
    /// `f1 - f2` product: conjugated, except the
    /// [`Self::unconjugated_in_f1_minus_f2`] pairs.
    fn minus_f2_value(&self, unknowns: &MnaUnknowns, x: &[Complex]) -> Complex {
        self.signed(unknowns, x).conj()
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
    /// A charge polynomial in the same controls whose `j omega` current C
    /// folds into this (current) term's value before loading it
    /// (`diodisto.c`'s `temp += -omega * D1i(...)`), set by
    /// [`Self::with_charge`].
    pub charge: Option<Taylor>,
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
            charge: None,
        }
    }

    /// Folds the charge polynomial `charge` into this term as `diodisto.c`
    /// does (see [`Self::charge`]).
    #[must_use]
    pub fn with_charge(mut self, charge: Taylor) -> Self {
        self.charge = Some(charge);
        self
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
    // CKTdisto visits the devices in CKTload order, which fixes the order the
    // terms accumulate into each right-hand-side entry.
    for index in circuit.load_order() {
        let device = &circuit.devices()[index];
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
                    if !term.taylor.is_finite() || !term.charge.is_none_or(|c| c.is_finite()) {
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

/// A term's control values in the kernels a [`Product`] reads.
enum KernelValues {
    TwoF1(dloadfns::Values),
    ThreeF1(dloadfns::Values, dloadfns::Values),
    F12(dloadfns::Values, dloadfns::Values),
    TwoF1MinusF2(
        dloadfns::Values,
        dloadfns::Values,
        dloadfns::Values,
        dloadfns::Values,
    ),
}

impl KernelValues {
    fn evaluate(&self, t: &Taylor) -> Complex {
        match *self {
            Self::TwoF1(x) => dloadfns::two_f1(t, x),
            Self::ThreeF1(x, x2) => dloadfns::three_f1(t, x, x2),
            Self::F12(x, b) => dloadfns::f12(t, x, b),
            Self::TwoF1MinusF2(x, b, x2, m) => dloadfns::two_f1_minus_f2(t, x, b, x2, m),
        }
    }
}

/// Adds every term's contribution to `rhs` for `product` (C `DEVdisto` with
/// the `dloadfns.c` polynomial evaluators, [`dloadfns`]): with `X`, `B`,
/// `X2`, `M` the control values in `H1(f1)`, `H1(+-f2)`, `H2(f1,f1)` and
/// `H2(f1,-f2)`,
///
/// | product | value of `c_ij v_i v_j` / `c_ijk v_i v_j v_k` |
/// | --- | --- |
/// | `2f1` | `X_i X_j` / 0 |
/// | `3f1` | `X_i X2_j + X_j X2_i` / `X_i X_j X_k` |
/// | `f1 +- f2` | `(X_i B_j + X_j B_i) / 2` / 0 |
/// | `2f1 - f2` | `(2 (X_i M_j + X_j M_i) + B_i X2_j + B_j X2_i) / 3` / `(X_i X_j B_k + X_i X_k B_j + X_j X_k B_i) / 3` |
///
/// A charge's value is multiplied by `j omega`, `omega` being the response's
/// angular frequency (negative for `f1 - f2` below `f2`, as in C). The sums
/// are evaluated in C's operand order and grouping, so the right-hand side
/// rounds exactly as C's does.
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
        let values = |kernel: &[Complex]| -> dloadfns::Values {
            let mut v = [Complex::ZERO; 3];
            for (slot, control) in v.iter_mut().zip(&term.controls) {
                *slot = control.value(unknowns, kernel);
            }
            v
        };
        let minus_f2 = |kernel: &[Complex]| -> dloadfns::Values {
            let mut v = [Complex::ZERO; 3];
            for (slot, control) in v.iter_mut().zip(&term.controls) {
                *slot = control.minus_f2_value(unknowns, kernel);
            }
            v
        };
        let x = values(kernels.h1);
        let inputs = match product {
            Product::TwoF1 => KernelValues::TwoF1(x),
            Product::ThreeF1 => KernelValues::ThreeF1(x, values(kernels.h2.ok_or_else(missing)?)),
            Product::F1PlusF2 => KernelValues::F12(x, values(kernels.h1_f2.ok_or_else(missing)?)),
            Product::F1MinusF2 => {
                KernelValues::F12(x, minus_f2(kernels.h1_f2.ok_or_else(missing)?))
            }
            Product::TwoF1MinusF2 => {
                let b = values(kernels.h1_f2.ok_or_else(missing)?).map(Complex::conj);
                let x2 = if term.first_order_im3_kernel {
                    x
                } else {
                    values(kernels.h2.ok_or_else(missing)?)
                };
                let m = values(kernels.h2_minus.ok_or_else(missing)?);
                KernelValues::TwoF1MinusF2(x, b, x2, m)
            }
        };
        let value = inputs.evaluate(&term.taylor);
        // A charge's current is `j omega` times its polynomial: C's `temp =
        // -omega * DFi(...)`, `itemp = omega * DFn(...)`.
        let (mut temp, mut itemp) = match term.response {
            Response::Current => (value.re, value.im),
            Response::Charge => (-omega * value.im, omega * value.re),
        };
        if let Some(charge) = &term.charge {
            // diodisto.c: `temp += -omega * D1i(...)`, `itemp += omega *
            // D1n(...)` onto the junction current's polynomial.
            let charge = inputs.evaluate(charge);
            temp += -omega * charge.im;
            itemp += omega * charge.re;
        }
        let [a, b] = term.nodes;
        if let Some(row) = unknowns.node_row(a) {
            rhs[row].re -= temp;
            rhs[row].im -= itemp;
        }
        if let Some(row) = unknowns.node_row(b) {
            rhs[row].re += temp;
            rhs[row].im += itemp;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Taylor;
    use super::deriv::Dderivs;

    /// A variable at `value` (C's `d_p` with its value set).
    fn var(index: usize, value: f64) -> Dderivs {
        let mut d = Dderivs::variable(index);
        d.value = value;
        d
    }

    /// Checks a derivative structure against central finite differences of
    /// its closed form.
    fn check(d: &Dderivs, f: impl Fn(f64, f64, f64) -> f64, at: [f64; 3]) {
        let h = 1e-3;
        let point = |dp: f64, dq: f64| f(at[0] + dp, at[1] + dq, at[2]);
        let value = point(0., 0.);
        assert!((d.value - value).abs() <= 1e-12 * value.abs().max(1.));
        let wanted = [
            (d.d1_p, (point(h, 0.) - point(-h, 0.)) / (2. * h)),
            (d.d1_q, (point(0., h) - point(0., -h)) / (2. * h)),
            (
                d.d2_p2,
                (point(h, 0.) - 2. * value + point(-h, 0.)) / (h * h),
            ),
            (
                d.d2_pq,
                (point(h, h) - point(h, -h) - point(-h, h) + point(-h, -h)) / (4. * h * h),
            ),
            (
                d.d3_p3,
                (point(2. * h, 0.) - 2. * point(h, 0.) + 2. * point(-h, 0.) - point(-2. * h, 0.))
                    / (2. * h * h * h),
            ),
        ];
        for (index, (got, want)) in wanted.into_iter().enumerate() {
            assert!(
                (got - want).abs() <= 1e-4 * want.abs().max(1e-3),
                "{index}: {got} vs {want}"
            );
        }
    }

    #[test]
    fn derivative_arithmetic_matches_closed_forms() {
        let at = [0.3, 0.7, -0.2];
        let p = var(0, at[0]);
        let q = var(1, at[1]);
        let r = var(2, at[2]);
        let mut q1 = q;
        q1.value += 1.;
        let f = p.mul(&q).plus(&r).exp().div(&q1.sqrt());
        check(&f, |p, q, r| (p * q + r).exp() / (q + 1.).sqrt(), at);
        let g = p.times(2.).tan().mul(&p.inv());
        check(&g, |p, _, _| (2. * p).tan() / p, at);
        // The mixed third derivative of p q r is exactly 1.
        assert_eq!(p.mul(&q).mul(&r).d3_pqr, 1.);
        // sqrtder.c: all derivatives vanish at a zero argument.
        assert_eq!(Dderivs::default().sqrt(), Dderivs::default());
        let t = Taylor::single(2., 3.);
        assert_eq!((t.xx, t.xxx, t.yy), (2., 3., 0.));
    }
}
