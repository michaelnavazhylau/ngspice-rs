//! Roots of `det(A + s E) = 0`: ngspice's pole-zero root search.
//!
//! A port of `CKTpzFindZeros` and its helpers
//! (`src/spicelib/analysis/cktpzstr.c`) with `NIpzSym2`/`NIpzMuller`
//! (`src/maths/ni/nipzmeth.c`). Each trial loads `A + s E` into one SPARSE
//! matrix (`maths::sparse13`), reorders it as `SMPcReorder(1e-30, 0)`
//! does (keeping the previous pivots when they pass the threshold test) and
//! takes the determinant (`SMPcDProd`: SPARSE's base-10 mantissa converted to
//! a base-2 mantissa/exponent pair). The search walks the real axis with the
//! trial list kept sorted by `s`, brackets sign changes with a deflated
//! quadratic fit (`NIpzSym2`), deflates every found root out of the stored
//! trials (`CKTpzVerify`), and continues into the complex plane with Muller's
//! method from the curvature hint `NIpzK`.
//!
//! C's file-scope state (`CKTpzTrapped`, `NIpzK`, the guess bounds, move
//! history) lives in `Search`; `Last_Move` starts at zero for each search
//! instead of carrying over from a previous one. C's warnings (iteration limit,
//! numerical aberrations) go to standard error and the roots found so far are
//! returned, as in C; a determinant that vanishes at every trial is C's
//! `E_SHORT`.

use crate::maths::Matrix;
use crate::maths::linear::numerical;
use crate::maths::sparse13::{Sparse, Status};
use crate::primitives::{Complex, Real, SpiceResult};

/// The roots of a pencil, see [`pencil_roots`].
#[derive(Debug, Clone, PartialEq)]
pub struct PencilRoots {
    /// Roots in `PZpost` order: ascending real part, then imaginary part,
    /// repeated by multiplicity, each complex root followed by its conjugate.
    pub roots: Vec<Complex>,
}

/// One device's pole-zero contribution to one matrix element, as a C
/// `DEVpzLoad` adds it: `*elt += g + c * s.real` and `*(elt + 1) += c *
/// s.imag` (`diopzld.c`), `*elt += g` alone (`respzld.c`) or `c * s.real`
/// alone (`cappzld.c`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PencilTerm {
    /// Zero-based matrix row.
    pub row: usize,
    /// Zero-based matrix column.
    pub col: usize,
    /// The `s`-independent part `g`, if the device adds one.
    pub conductance: Option<Real>,
    /// The coefficient `c` of `s`, if the device adds one.
    pub capacitance: Option<Real>,
}

impl PencilTerm {
    /// The value this term adds at `s`.
    fn at(&self, s: Cx) -> Complex {
        match (self.conductance, self.capacitance) {
            (Some(g), Some(c)) => Complex::new(g + c * s.re, c * s.im),
            (Some(g), None) => Complex::new(g, 0.0),
            (None, Some(c)) => Complex::new(c * s.re, c * s.im),
            (None, None) => Complex::ZERO,
        }
    }
}

/// The matrix `CKTpzSetup` builds and `CKTpzLoad` (`cktpzld.c`) loads at
/// each trial `s`: the elements are created in order (the circuit's setup
/// elements, then the drive), every trial clears them, adds the device
/// [`PencilTerm`]s in load order, adds the solution column to the balance
/// column (`SMPcAddCol`), zeroes the solution column (`SMPcZeroCol`) and
/// stores the unit drive `+1`/`-1` in the input rows of the solution column.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PencilLoad {
    /// The matrix order.
    pub order: usize,
    /// Zero-based `(row, col)` elements to create before loading, in
    /// creation order (SPARSE numbers rows and columns by first appearance).
    /// Term and drive positions not listed are created after these.
    pub elements: Vec<(usize, usize)>,
    /// Device contributions in load order.
    pub terms: Vec<PencilTerm>,
    /// The solution column (`PZsolution_col`), if any.
    pub solution: Option<usize>,
    /// The balance column (`PZbalance_col`), if any.
    pub balance: Option<usize>,
    /// The rows receiving `+1` and `-1` in the solution column
    /// (`PZdrive_pptr`/`PZdrive_nptr`).
    pub drive: [Option<usize>; 2],
    /// The relative pivot threshold the matrix holds. `CKTpzFindZeros`
    /// reorders with `SMPcReorder(matrix, 1e-30, 0.0)`, and a relative
    /// threshold outside `(0, 1]` keeps the one `ckt->CKTmatrix` stores: the
    /// `.options pivrel` (`CKTpivotRelTol`) of the operating point's
    /// `SMPreorder`. `0` keeps SPARSE's default, `1e-3`.
    pub relative_threshold: f64,
}

impl PencilLoad {
    /// A load of the plain pencil `A + s E` with one term per nonzero
    /// position and the elements created row by row.
    ///
    /// # Errors
    /// Non-square or mismatched matrices.
    pub fn from_dense(a: &Matrix, e: &Matrix) -> SpiceResult<Self> {
        let n = a.rows();
        if a.cols() != n || e.rows() != n || e.cols() != n {
            return Err(numerical(
                "pole-zero search",
                format!(
                    "needs square matrices of one size, got {}x{} and {}x{}",
                    a.rows(),
                    a.cols(),
                    e.rows(),
                    e.cols()
                ),
            ));
        }
        let mut terms = Vec::new();
        for row in 0..n {
            for col in 0..n {
                let g = a.get(row, col).unwrap_or(0.0);
                let c = e.get(row, col).unwrap_or(0.0);
                if g != 0.0 || c != 0.0 {
                    terms.push(PencilTerm {
                        row,
                        col,
                        conductance: Some(g),
                        capacitance: (c != 0.0).then_some(c),
                    });
                }
            }
        }
        Ok(Self {
            order: n,
            elements: terms.iter().map(|t| (t.row, t.col)).collect(),
            terms,
            ..Self::default()
        })
    }

    fn validate(&self) -> SpiceResult<()> {
        let n = self.order;
        if n == 0 {
            return Err(numerical("pole-zero search", "needs a nonempty matrix"));
        }
        let inside = |i: usize| i < n;
        let positions = self
            .elements
            .iter()
            .copied()
            .chain(self.terms.iter().map(|t| (t.row, t.col)));
        if !positions.into_iter().all(|(r, c)| inside(r) && inside(c))
            || ![self.solution, self.balance, self.drive[0], self.drive[1]]
                .into_iter()
                .flatten()
                .all(inside)
        {
            return Err(numerical(
                "pole-zero search",
                "matrix position out of range",
            ));
        }
        if self.terms.iter().any(|t| {
            !t.conductance.unwrap_or(0.0).is_finite() || !t.capacitance.unwrap_or(0.0).is_finite()
        }) {
            return Err(numerical("pole-zero search", "non-finite matrix entry"));
        }
        Ok(())
    }
}

const NITER_LIM: usize = 200;

const SHIFT_LEFT: u8 = 2;
const SHIFT_RIGHT: u8 = 3;
const SKIP_LEFT: u8 = 4;
const SKIP_RIGHT: u8 = 5;
const INIT: u8 = 6;
const GUESS: u8 = 7;
const SPLIT_LEFT: u8 = 8;
const SPLIT_RIGHT: u8 = 9;
const MULLER: u8 = 10;
const SYM: u8 = 11;
const SYM2: u8 = 12;
const COMPLEX_INIT: u8 = 13;
const COMPLEX_GUESS: u8 = 14;

const NEAR_LEFT: u8 = 4;
const MID_LEFT: u8 = 5;
const FAR_LEFT: u8 = 6;
const NEAR_RIGHT: u8 = 7;
const FAR_RIGHT: u8 = 8;
const MID_RIGHT: u8 = 9;

const ISAROOT: u8 = 2;
const ISAREPEAT: u8 = 4;
const ISANABERRATION: u8 = 8;
const ISAMINIMA: u8 = 16;

/// `SPcomplex` arithmetic with the exact C macro formulas (`complex.h`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Cx {
    re: Real,
    im: Real,
}

impl Cx {
    const ZERO: Self = Self { re: 0.0, im: 0.0 };
    fn new(re: Real, im: Real) -> Self {
        Self { re, im }
    }
    fn scale(self, k: Real) -> Self {
        Self::new(k * self.re, k * self.im)
    }
    /// `C_SUBEQ`.
    fn sub(self, b: Self) -> Self {
        Self::new(self.re - b.re, self.im - b.im)
    }
    /// `C_ADDEQ`.
    fn add(self, b: Self) -> Self {
        Self::new(self.re + b.re, self.im + b.im)
    }
    /// `C_MULEQ` / `C_MUL`.
    fn mul(self, b: Self) -> Self {
        Self::new(
            self.re * b.re - self.im * b.im,
            self.re * b.im + self.im * b.re,
        )
    }
    /// `C_DIV` / `C_DIVEQ`.
    fn div(self, b: Self) -> Self {
        let mag = b.re * b.re + b.im * b.im;
        Self::new(
            (self.re * b.re + self.im * b.im) / mag,
            (-self.re * b.im + self.im * b.re) / mag,
        )
    }
    /// `C_MAG2`: squared modulus as the real part.
    fn mag2(self) -> Self {
        Self::new(self.re * self.re + self.im * self.im, 0.0)
    }
    /// `C_SQRT`.
    fn sqrt(self) -> Self {
        if self.im == 0.0 {
            if self.re < 0.0 {
                Self::new(0.0, (-self.re).sqrt())
            } else {
                Self::new(self.re.sqrt(), 0.0)
            }
        } else {
            let mag = self.re.hypot(self.im);
            let a = (mag - self.re) / 2.0;
            if a <= 0.0 {
                let re = mag.sqrt();
                Self::new(re, self.im / (2.0 * re))
            } else {
                let a = a.sqrt();
                Self::new(self.im / (2.0 * a), a)
            }
        }
    }
    fn is_zero(self) -> bool {
        self.re == 0.0 && self.im == 0.0
    }
}

/// `C_NORM`: scale `a` into `(0.5, 1]` per component, tracking the binary
/// exponent in `mag` (set to zero for a zero value).
fn c_norm(a: &mut Cx, mag: &mut i32) {
    if a.is_zero() {
        *mag = 0;
        return;
    }
    while a.re.abs() > 1.0 || a.im.abs() > 1.0 {
        *mag += 1;
        a.re /= 2.0;
        a.im /= 2.0;
    }
    while a.re.abs() <= 0.5 && a.im.abs() <= 0.5 {
        *mag -= 1;
        a.re *= 2.0;
        a.im *= 2.0;
    }
}

/// `R_NORM`.
fn r_norm(a: &mut Real, mag: &mut i32) {
    if *a == 0.0 {
        *mag = 0;
        return;
    }
    while a.abs() > 1.0 {
        *mag += 1;
        *a /= 2.0;
    }
    while a.abs() < 0.5 {
        *mag -= 1;
        *a *= 2.0;
    }
}

/// `zaddeq`: `a * 2^amag = x * 2^xmag + y * 2^ymag`, normalized.
fn zaddeq(x: Real, xmag: i32, y: Real, ymag: i32) -> (Real, i32) {
    let (mut x, mut y) = (x, y);
    let mut amag;
    if xmag > ymag {
        amag = xmag;
        if xmag > 50 + ymag {
            y = 0.0;
        } else {
            for _ in 0..(xmag - ymag) {
                y /= 2.0;
            }
        }
    } else {
        amag = ymag;
        if ymag > 50 + xmag {
            x = 0.0;
        } else {
            for _ in 0..(ymag - xmag) {
                x /= 2.0;
            }
        }
    }
    let mut a = x + y;
    if a == 0.0 {
        amag = 0;
    } else {
        while a.abs() > 1.0 {
            a /= 2.0;
            amag += 1;
        }
        while a.abs() < 0.5 {
            a *= 2.0;
            amag -= 1;
        }
    }
    (a, amag)
}

fn sgn(x: Real) -> i32 {
    if x < 0.0 {
        -1
    } else if x == 0.0 {
        0
    } else {
        1
    }
}

/// C `logb`: the unbiased binary exponent of a nonzero finite value.
fn logb(x: Real) -> Real {
    let exponent = ((x.abs().to_bits() >> 52) & 0x7ff) as i32;
    if exponent == 0 {
        x.abs().log2().floor()
    } else {
        Real::from(exponent - 1023)
    }
}

/// `SMPcDProd`: SPARSE's `mantissa * 10^p` as a base-2 mantissa/exponent.
fn base2_determinant(mantissa: Complex, p: i32) -> (Cx, i32) {
    let (mut re, mut im) = (mantissa.re, mantissa.im);
    let mut y = Real::from(p) * std::f64::consts::LN_10 / std::f64::consts::LN_2;
    let x = y.trunc();
    y -= x;
    let z = 2.0_f64.powf(y);
    re *= z;
    im *= z;
    let (mut y, z) = if re != 0.0 {
        (logb(re), if im != 0.0 { logb(im) } else { 0.0 })
    } else if im != 0.0 {
        (0.0, logb(im))
    } else {
        (0.0, 0.0)
    };
    if y < z {
        y = z;
    }
    #[allow(clippy::cast_possible_truncation)]
    let exponent = (x + y) as i32;
    #[allow(clippy::cast_possible_truncation)]
    let shift = 2.0_f64.powi(-(y as i32));
    (Cx::new(re * shift, im * shift), exponent)
}

#[derive(Debug, Clone)]
struct Trial {
    s: Cx,
    f_def: Cx,
    mag_def: i32,
    multiplicity: u32,
    flags: u8,
    count: u32,
    next: Option<usize>,
    prev: Option<usize>,
}

type Set = [Option<usize>; 3];

/// The SPARSE matrix, its element per term and its drive elements.
type Loaded = (Sparse<Complex>, Vec<usize>, [Option<usize>; 2]);

/// One `CKTpzFindZeros` run with its file-scope state.
struct Search<'a> {
    load: &'a PencilLoad,
    sparse: Option<Loaded>,
    structurally_singular: bool,
    trials: Vec<Trial>,
    head: Option<usize>,
    zero_trial: Option<usize>,
    nzeros: usize,
    nflat: usize,
    max_zeros: usize,
    seq_num: usize,
    guess_param: Real,
    high_guess: Real,
    low_guess: Real,
    last_move: u8,
    consec_moves: u32,
    niter: usize,
    aberr_num: usize,
    trapped: u8,
    pz_k: Real,
    pz_k_mag: i32,
}

/// Roots of `det(A + s E)` found by ngspice's pole-zero search
/// ([`PencilLoad::from_dense`] + [`pencil_load_roots`]).
///
/// # Errors
/// As [`pencil_load_roots`], and non-square or mismatched matrices.
pub fn pencil_roots(a: &Matrix, e: &Matrix) -> SpiceResult<PencilRoots> {
    pencil_load_roots(&PencilLoad::from_dense(a, e)?)
}

/// Roots of the determinant of `load` as a function of `s`, found by
/// ngspice's pole-zero search (`CKTpzFindZeros`).
///
/// # Errors
/// An empty matrix, out-of-range positions or non-finite terms, a
/// determinant that vanishes at every trial (C `E_SHORT`, reported as a
/// "singular pencil"), or the search losing numerical stability (C
/// `E_PANIC`).
pub fn pencil_load_roots(load: &PencilLoad) -> SpiceResult<PencilRoots> {
    load.validate()?;
    let n = load.order;
    let mut search = Search {
        load,
        sparse: None,
        structurally_singular: false,
        trials: Vec::new(),
        head: None,
        zero_trial: None,
        nzeros: 0,
        nflat: 0,
        max_zeros: n,
        seq_num: 1,
        guess_param: 0.0,
        high_guess: -1.0,
        low_guess: 1.0,
        last_move: 0,
        consec_moves: 0,
        niter: 0,
        aberr_num: 0,
        trapped: 0,
        pz_k: 0.0,
        pz_k_mag: 0,
    };
    search.find_zeros().map(|roots| PencilRoots { roots })
}

impl Search<'_> {
    fn s(&self, i: Option<usize>) -> Cx {
        self.trials[i.expect("C dereferences this neighborhood entry")].s
    }

    /// `CKTpzFindZeros`.
    fn find_zeros(&mut self) -> SpiceResult<Vec<Complex>> {
        let mut set: Set = [None; 3];
        self.reset(&mut set);
        loop {
            let mut strat;
            loop {
                strat = self.strat(&set)?;
                if !(strat < GUESS && self.trapped == 0) {
                    break;
                }
                if !self.step(strat, &mut set) {
                    strat = GUESS;
                    break;
                }
            }
            self.niter += 1;
            let candidate = self.eval(strat, &set)?;
            let new = self.run_trial(candidate, &set)?;
            let flags = self.trials[new].flags;
            if flags & ISAROOT != 0 {
                if self.verify(new) {
                    self.niter = 0;
                    self.reset(&mut set);
                } else {
                    self.update_set(&mut set, new);
                }
            } else if flags & ISANABERRATION != 0 {
                self.reset(&mut set);
                self.aberr_num += 1;
            } else if flags & ISAMINIMA != 0 {
                set = [None, Some(new), None];
            } else {
                self.update_set(&mut set, new);
            }
            let spread = self.high_guess - self.low_guess;
            let open = set[0].is_none()
                || set[2].is_none()
                || self.trapped != 0
                || self.s(set[2]).re - self.s(set[0]).re < 1e22;
            if !(spread < 1e40
                && self.nzeros < self.max_zeros
                && self.niter < NITER_LIM
                && self.aberr_num < 3
                && spread < 1e35
                && open)
            {
                break;
            }
        }
        if self.nzeros + 1 >= self.seq_num {
            return Err(numerical(
                "pole-zero search",
                "singular pencil: the determinant vanished at every trial (C E_SHORT, \
                 \"The input signal is shorted on the way to the output\")",
            ));
        }
        if self.aberr_num > 2 {
            eprintln!(
                "warning: Pole-zero converging to numerical aberrations; giving up after {} trials",
                self.seq_num
            );
        }
        if self.niter >= NITER_LIM {
            eprintln!(
                "warning: Pole-zero iteration limit reached; giving up after {} trials",
                self.seq_num
            );
        }
        // clear_trials(0) and PZpost.
        let mut roots = Vec::new();
        let mut t = self.head;
        while let Some(i) = t {
            let trial = &self.trials[i];
            if trial.flags & ISAROOT != 0 {
                for _ in 0..trial.multiplicity {
                    roots.push(Complex::new(trial.s.re, trial.s.im));
                    if trial.s.im != 0.0 {
                        roots.push(Complex::new(trial.s.re, -trial.s.im));
                    }
                }
            }
            t = trial.next;
        }
        Ok(roots)
    }

    /// `PZeval`: the next trial point for a strategy (not yet in the list).
    fn eval(&mut self, strat: u8, set: &Set) -> SpiceResult<usize> {
        let mut trial = Trial {
            s: Cx::ZERO,
            f_def: Cx::ZERO,
            mag_def: 0,
            multiplicity: 0,
            flags: 0,
            count: 0,
            next: None,
            prev: None,
        };
        self.seq_num += 1;
        match strat {
            GUESS => {
                if self.high_guess < self.low_guess {
                    self.guess_param = 0.0;
                } else if self.guess_param > 0.0 {
                    self.guess_param = if self.high_guess > 0.0 {
                        self.high_guess * 10.0
                    } else {
                        1.0
                    };
                } else {
                    self.guess_param = if self.low_guess < 0.0 {
                        self.low_guess * 10.0
                    } else {
                        -1.0
                    };
                }
                if self.high_guess < self.guess_param {
                    self.high_guess = self.guess_param;
                }
                if self.low_guess > self.guess_param {
                    self.low_guess = self.guess_param;
                }
                trial.s.re = self.guess_param;
                trial.s.im = set[1].map_or(0.0, |i| self.trials[i].s.im);
            }
            SYM | SYM2 => {
                trial.s = Cx::new(self.sym2(set)?, 0.0);
                let (s0, s1) = (self.s(set[0]), self.s(set[1]));
                match self.trapped {
                    1 => {
                        if trial.s.re < s0.re || trial.s.re > s1.re {
                            trial.s.re = (s0.re + s1.re) / 2.0;
                        }
                    }
                    2 => {
                        let s2 = self.s(set[2]);
                        if trial.s.re < s1.re || trial.s.re > s2.re {
                            trial.s.re = (s1.re + s2.re) / 2.0;
                        }
                    }
                    3 => {
                        let s2 = self.s(set[2]);
                        if trial.s.re <= s0.re
                            || (trial.s.re == s1.re && trial.s.im == s1.im)
                            || trial.s.re >= s2.re
                        {
                            trial.s.re = (s0.re + s2.re) / 2.0;
                            if trial.s.re == s1.re {
                                trial.s.re =
                                    if self.last_move == MID_LEFT || self.last_move == NEAR_RIGHT {
                                        (s0.re + s1.re) / 2.0
                                    } else {
                                        (s1.re + s2.re) / 2.0
                                    };
                            }
                        }
                    }
                    _ => {}
                }
            }
            COMPLEX_INIT => {
                trial.s.re = self.s(set[1]).re;
                if self.pz_k != 0.0 && self.pz_k_mag > -10 {
                    while self.pz_k_mag > 0 {
                        self.pz_k *= 2.0;
                        self.pz_k_mag -= 1;
                    }
                    while self.pz_k_mag < 0 {
                        self.pz_k /= 2.0;
                        self.pz_k_mag += 1;
                    }
                    trial.s.im = self.pz_k;
                } else {
                    trial.s.im = 10000.0;
                }
                self.pz_k = 0.0;
                self.pz_k_mag = 0;
            }
            COMPLEX_GUESS => {
                trial.s.re = self.s(set[0]).re;
                trial.s.im = if set[2].is_none() { 1.0e8 } else { 1.0e12 };
            }
            MULLER => trial.s = self.muller(set),
            SPLIT_LEFT => {
                trial.s.re = (self.s(set[0]).re + 2.0 * self.s(set[1]).re) / 3.0;
            }
            SPLIT_RIGHT => {
                trial.s.re = (self.s(set[2]).re + 2.0 * self.s(set[1]).re) / 3.0;
            }
            _ => return Err(numerical("pole-zero search", "Step type unknown")),
        }
        self.trials.push(trial);
        Ok(self.trials.len() - 1)
    }

    /// `CKTpzStrat`.
    #[allow(clippy::needless_late_init)] // keeps C's branch structure
    fn strat(&mut self, set: &Set) -> SpiceResult<u8> {
        let mut new_trap = 0;
        let suggestion;
        let minima = set[1].is_some_and(|i| self.trials[i].flags & ISAMINIMA != 0);
        if minima {
            suggestion = COMPLEX_INIT;
        } else if set[0].is_some_and(|i| self.trials[i].s.im != 0.0) {
            suggestion = if set[1].is_none() || set[2].is_none() {
                COMPLEX_GUESS
            } else {
                MULLER
            };
        } else if set.iter().any(Option::is_none) {
            suggestion = INIT;
        } else {
            let t0 = &self.trials[set[0].unwrap_or_default()];
            let t1 = &self.trials[set[1].unwrap_or_default()];
            let t2 = &self.trials[set[2].unwrap_or_default()];
            let mut chosen;
            if sgn(t0.f_def.re) != sgn(t1.f_def.re) {
                new_trap = 1;
                chosen = SYM2;
            } else if sgn(t1.f_def.re) != sgn(t2.f_def.re) {
                new_trap = 2;
                chosen = SYM2;
            } else {
                let (a, a_mag) = zaddeq(t1.f_def.re, t1.mag_def, -t0.f_def.re, t0.mag_def);
                let (b, b_mag) = zaddeq(t2.f_def.re, t2.mag_def, -t1.f_def.re, t1.mag_def);
                if self.trapped == 0 {
                    let k1 = t1.s.re - t0.s.re;
                    let k2 = t2.s.re - t1.s.re;
                    chosen = if a_mag + 10 < t0.mag_def
                        && a_mag + 10 < t1.mag_def
                        && b_mag + 10 < t1.mag_def
                        && b_mag + 10 < t2.mag_def
                    {
                        if k1 > k2 { SKIP_RIGHT } else { SKIP_LEFT }
                    } else if sgn(a) != -sgn(b) {
                        if a == 0.0 {
                            SKIP_LEFT
                        } else if b == 0.0 {
                            SKIP_RIGHT
                        } else if sgn(a) == sgn(t1.f_def.re) {
                            SHIFT_LEFT
                        } else {
                            SHIFT_RIGHT
                        }
                    } else if sgn(a) == -sgn(t1.f_def.re) {
                        new_trap = 3;
                        SYM
                    } else if k1 > k2 {
                        SKIP_RIGHT
                    } else {
                        SKIP_LEFT
                    };
                } else {
                    new_trap = 3;
                    chosen = if sgn(a) != sgn(b) {
                        SYM
                    } else if a_mag > b_mag || (a_mag == b_mag && a.abs() > b.abs()) {
                        SPLIT_LEFT
                    } else {
                        SPLIT_RIGHT
                    };
                }
            }
            if self.consec_moves >= 3 && self.trapped == new_trap {
                new_trap = self.trapped;
                chosen = if self.last_move == MID_LEFT || self.last_move == NEAR_RIGHT {
                    SPLIT_LEFT
                } else if self.last_move == MID_RIGHT || self.last_move == NEAR_LEFT {
                    SPLIT_RIGHT
                } else {
                    return Err(numerical(
                        "pole-zero search",
                        "trapped search has no usable last move (C calls controlled_exit)",
                    ));
                };
                self.consec_moves = 0;
            }
            suggestion = chosen;
        }
        self.trapped = new_trap;
        Ok(suggestion)
    }

    /// Loads the matrix at `s` and returns SPARSE's determinant
    /// (`CKTpzLoad`, `SMPcReorder(1e-30, 0)`, `SMPcDProd`), or `None` when
    /// singular. The matrix is created on the first trial, unordered (C
    /// runs no `SMPpreOrder` for SPARSE), and keeps its pivot order between
    /// trials while the pivots pass the threshold test.
    fn determinant_at(&mut self, s: Cx) -> Option<(Cx, i32)> {
        if self.structurally_singular {
            return None;
        }
        let load = self.load;
        if self.sparse.is_none() {
            let mut sparse = Sparse::new(load.order);
            for &(row, col) in &load.elements {
                sparse.get_element(row + 1, col + 1);
            }
            let handles = load
                .terms
                .iter()
                .map(|t| sparse.get_element(t.row + 1, t.col + 1))
                .collect();
            let drive = match load.solution {
                Some(col) => load
                    .drive
                    .map(|row| row.map(|row| sparse.get_element(row + 1, col + 1))),
                None => [None, None],
            };
            if !sparse.fully_translated() {
                self.structurally_singular = true;
                return None;
            }
            self.sparse = Some((sparse, handles, drive));
        }
        let (sparse, handles, drive) = self.sparse.as_mut()?;
        sparse.clear();
        for (term, &handle) in load.terms.iter().zip(handles.iter()) {
            sparse.add(handle, term.at(s));
        }
        if let Some(solution) = load.solution {
            if let Some(balance) = load.balance {
                sparse.add_col(balance + 1, solution + 1);
            }
            sparse.zero_col(solution + 1);
        }
        for (element, value) in drive.iter().zip([1.0, -1.0]) {
            if let Some(element) = *element {
                let im = sparse.value(element).im;
                sparse.set(element, Complex::new(value, im));
            }
        }
        match sparse.order_and_factor(load.relative_threshold, 1.0e-30) {
            Status::Singular | Status::ZeroDiag => None,
            Status::Okay | Status::SmallPivot => {
                let (mantissa, exponent) = sparse.determinant();
                Some(base2_determinant(mantissa, exponent))
            }
        }
    }

    /// `CKTpzRunTrial`: evaluates (or merges) a trial and links it into the
    /// sorted list. Returns the trial now standing for it.
    fn run_trial(&mut self, candidate: usize, set: &Set) -> SpiceResult<usize> {
        let mut new = candidate;
        if self.trials[new].s.im < 0.0 {
            self.trials[new].s.im *= -1.0;
        }
        let mut pretest = false;
        let mut shifted = false;
        let mut repeat = false;
        let mut def_frac;
        let mut def_mag;
        let mut prev;
        let mut matched;
        let mut hit = None;
        loop {
            def_mag = 0;
            def_frac = Cx::new(1.0, 0.0);
            let was_shifted = shifted;
            shifted = false;
            prev = None;
            matched = None;
            let mut p = self.head;
            while let Some(pi) = p {
                let diff = self.trials[pi].s.sub(self.trials[new].s);
                if diff.re < 0.0 || (diff.re == 0.0 && diff.im < 0.0) {
                    prev = Some(pi);
                }
                let root = self.trials[pi].flags & ISAROOT != 0;
                let (abstol, reltol) = if root { (1e-5, 1e-6) } else { (1e-20, 1e-12) };
                if diff.im == 0.0
                    && diff.re.abs() / (self.trials[pi].s.re.abs() + abstol / reltol) < reltol
                {
                    if was_shifted
                        || self.trials[pi].count >= 3
                        || !self.alter(new, set[1], abstol, reltol)
                    {
                        self.trials[pi].count = 0;
                        pretest = true;
                        hit = Some(pi);
                        break;
                    }
                    self.trials[pi].count += 1;
                    shifted = true;
                    break;
                }
                if self.trapped == 0 {
                    self.trials[pi].count = 0;
                }
                if root {
                    let mut diff = diff;
                    let mut diff_mag = 0;
                    c_norm(&mut diff, &mut diff_mag);
                    if diff.im != 0.0 {
                        diff = diff.mag2();
                        diff_mag *= 2;
                    }
                    c_norm(&mut diff, &mut diff_mag);
                    for _ in 0..self.trials[pi].multiplicity {
                        def_frac = def_frac.mul(diff);
                        def_mag += diff_mag;
                        c_norm(&mut def_frac, &mut def_mag);
                    }
                } else if matched.is_none() {
                    matched = Some(pi);
                }
                p = self.trials[pi].next;
            }
            if !shifted {
                break;
            }
        }
        let mut singular = false;
        if pretest {
            let p = hit.unwrap_or(new);
            if self.trials[p].flags & ISAROOT == 0
                && self.trapped == 3
                && self.pz_k != 0.0
                && self.pz_k_mag > -10
            {
                self.trials[p].flags |= ISAMINIMA;
                new = p;
                repeat = true;
            } else if self.trials[p].flags & ISAROOT != 0 {
                new = p;
                self.trials[p].flags |= ISAREPEAT;
                self.trials[p].multiplicity += 1;
                repeat = true;
            } else {
                singular = true;
            }
        }
        if !repeat {
            let mut f_raw = Cx::ZERO;
            let mut mag_raw = 0;
            if !pretest {
                match self.determinant_at(self.trials[new].s) {
                    Some((f, mag)) => {
                        f_raw = f;
                        mag_raw = mag;
                    }
                    None => singular = true,
                }
            }
            let trial = &mut self.trials[new];
            if singular || f_raw.is_zero() {
                trial.f_def = Cx::ZERO;
                trial.mag_def = 0;
                trial.flags = ISAROOT;
            } else {
                // PZnumswaps is always 1 after SMPcReorder.
                trial.f_def = f_raw.div(def_frac);
                trial.mag_def = mag_raw - def_mag;
                c_norm(&mut trial.f_def, &mut trial.mag_def);
            }
            if let Some(prev) = prev {
                let after = self.trials[prev].next;
                self.trials[new].next = after;
                if let Some(after) = after {
                    self.trials[after].prev = Some(new);
                }
                self.trials[prev].next = Some(new);
            } else {
                match self.head {
                    Some(head) => self.trials[head].prev = Some(new),
                    None => self.zero_trial = Some(new),
                }
                self.trials[new].next = self.head;
                self.head = Some(new);
            }
            self.trials[new].prev = prev;
            if self.trials[new].flags & ISAROOT == 0 {
                match matched {
                    Some(m) => self.check_flat(m, new),
                    None => self.nflat = 1,
                }
            }
        }
        Ok(new)
    }

    /// `CKTpzVerify`: count the root, deflate every stored non-root trial
    /// by it and drop trials too close to it (always "verified").
    fn verify(&mut self, new: usize) -> bool {
        self.nzeros += 1;
        let s = self.trials[new].s;
        if s.im != 0.0 {
            self.nzeros += 1;
        }
        self.nflat = 0;
        if self.trials[new].multiplicity == 0 {
            self.trials[new].flags |= ISAROOT;
            self.trials[new].multiplicity = 1;
        }
        let mut prev: Option<usize> = None;
        let mut t = self.head;
        while let Some(ti) = t {
            let next = self.trials[ti].next;
            if self.trials[ti].flags & ISAROOT != 0 {
                prev = Some(ti);
                t = next;
                continue;
            }
            let mut diff = s.sub(self.trials[ti].s);
            if s.im != 0.0 {
                diff = diff.mag2();
            }
            let tdiff = diff.re;
            if diff.re != 0.0 {
                let mut diff_mag = 0;
                c_norm(&mut diff, &mut diff_mag);
                diff_mag *= -1;
                let trial = &mut self.trials[ti];
                trial.f_def = trial.f_def.div(diff);
                c_norm(&mut trial.f_def, &mut diff_mag);
                trial.mag_def += diff_mag;
            }
            if self.trials[ti].s.im != 0.0 || tdiff.abs() / (s.re.abs() + 200.0) < 0.005 {
                if let Some(p) = prev {
                    self.trials[p].next = next;
                }
                if let Some(n) = next {
                    self.trials[n].prev = prev;
                }
                if self.zero_trial == Some(ti) {
                    self.zero_trial = next.or(self.trials[ti].prev);
                }
                if self.head == Some(ti) {
                    self.head = next;
                }
            } else {
                match prev {
                    Some(p) => self.check_flat(p, ti),
                    None => self.nflat = 1,
                }
                self.trials[ti].flags &= !ISAMINIMA;
                prev = Some(ti);
            }
            t = next;
        }
        true
    }

    /// `pzseek`: the nearest non-root, non-minimum trial in a direction.
    fn seek(&mut self, t: Option<usize>, dir: i32) -> Option<usize> {
        self.guess_param = Real::from(dir);
        let mut t = t?;
        let skip = |trial: &Trial| trial.flags & (ISAROOT | ISAMINIMA) != 0;
        if dir == 0 && !skip(&self.trials[t]) {
            return Some(t);
        }
        loop {
            let next = if dir >= 0 {
                self.trials[t].next
            } else {
                self.trials[t].prev
            };
            t = next?;
            if !skip(&self.trials[t]) {
                return Some(t);
            }
        }
    }

    /// `CKTpzUpdateSet`.
    fn update_set(&mut self, set: &mut Set, new: usize) {
        let mut this_move = 0;
        let trial = self.trials[new].clone();
        let n = trial.s;
        if n.im != 0.0 {
            set[2] = set[1];
            set[1] = set[0];
            set[0] = Some(new);
        } else if set[1].is_none() {
            set[1] = Some(new);
        } else if set[2].is_none() && n.re > self.s(set[1]).re {
            set[2] = Some(new);
        } else if set[0].is_none() {
            set[0] = Some(new);
        } else if trial.flags & ISAMINIMA != 0 {
            set[1] = Some(new);
        } else if n.re < self.s(set[0]).re {
            set[2] = set[1];
            set[1] = set[0];
            set[0] = Some(new);
            this_move = FAR_LEFT;
        } else {
            let middle = &self.trials[set[1].unwrap_or_default()];
            let better = self.trapped == 0
                || trial.mag_def < middle.mag_def
                || (trial.mag_def == middle.mag_def
                    && trial.f_def.re.abs() < middle.f_def.re.abs());
            if n.re < middle.s.re {
                if better {
                    set[2] = set[1];
                    set[1] = Some(new);
                    this_move = MID_LEFT;
                } else {
                    set[0] = Some(new);
                    this_move = NEAR_LEFT;
                }
            } else if set[2].is_some() && n.re < self.s(set[2]).re {
                if better {
                    set[0] = set[1];
                    set[1] = Some(new);
                    this_move = MID_RIGHT;
                } else {
                    set[2] = Some(new);
                    this_move = NEAR_RIGHT;
                }
            } else {
                set[0] = set[1];
                set[1] = set[2];
                set[2] = Some(new);
                this_move = FAR_RIGHT;
            }
        }
        if self.trapped != 0 && this_move == self.last_move {
            self.consec_moves += 1;
        } else {
            self.consec_moves = 0;
        }
        self.last_move = this_move;
    }

    /// `check_flat`.
    fn check_flat(&mut self, a: usize, b: usize) {
        let (a, b) = (&self.trials[a], &self.trials[b]);
        let diff_mag = a.mag_def - b.mag_def;
        if diff_mag.abs() <= 1 {
            let mult = match diff_mag {
                1 => 2.0,
                -1 => 0.5,
                _ => 1.0,
            };
            let diff = a.f_def.scale(mult).sub(b.f_def).mag2();
            if diff.re < 1.0e-20 {
                self.nflat += 1;
            }
        }
    }

    /// `CKTpzStep`.
    fn step(&mut self, strat: u8, set: &mut Set) -> bool {
        match strat {
            INIT => {
                if set[1].is_none() {
                    set[1] = self.seek(self.zero_trial, 0);
                } else if set[2].is_none() {
                    set[2] = self.seek(set[1], 1);
                } else if set[0].is_none() {
                    set[0] = self.seek(set[1], -1);
                }
            }
            SKIP_LEFT => set[0] = self.seek(set[0], -1),
            SKIP_RIGHT => set[2] = self.seek(set[2], 1),
            SHIFT_LEFT => {
                set[2] = set[1];
                set[1] = set[0];
                set[0] = self.seek(set[0], -1);
            }
            SHIFT_RIGHT => {
                set[0] = set[1];
                set[1] = set[2];
                set[2] = self.seek(set[2], 1);
            }
            _ => {}
        }
        set.iter().all(Option::is_some)
    }

    /// `CKTpzReset`.
    fn reset(&mut self, set: &mut Set) {
        self.trapped = 0;
        self.consec_moves = 0;
        set[1] = self.seek(self.zero_trial, 0);
        if set[1].is_some() {
            set[0] = self.seek(set[1], -1);
            set[2] = self.seek(set[1], 1);
        } else {
            set[0] = None;
            set[2] = None;
        }
    }

    /// `alter`: move a trial that collides with an existing one.
    fn alter(&mut self, new: usize, nearto: Option<usize>, abstol: Real, reltol: Real) -> bool {
        let Some(near) = nearto else {
            return false;
        };
        let t = self.trials[near].clone();
        let neighbor = |i: Option<usize>| i.map(|i| self.trials[i].s.re);
        let (left, right) = (neighbor(t.prev), neighbor(t.next));
        let root = t.flags & ISAROOT != 0;
        let p1 = if self.trapped == 2 {
            t.s.re
        } else {
            let mut p1 = t.s.re;
            if root {
                p1 -= 1e-6 * t.s.re + 1e-5;
            }
            match left {
                Some(l) => p1 += l,
                None => p1 -= 10.0 * (p1.abs() + 1.0),
            }
            p1 / 2.0
        };
        let p2 = if self.trapped == 1 {
            t.s.re
        } else {
            let mut p2 = t.s.re;
            if root {
                p2 += 1e-6 * t.s.re + 1e-5;
            }
            match right {
                Some(r) => p2 += r,
                None => p2 += 10.0 * (p2.abs() + 1.0),
            }
            p2 / 2.0
        };
        // C's precedence: |p - x| / |x| + abstol / reltol < reltol.
        if left.is_some_and(|l| (p1 - l).abs() / l.abs() + abstol / reltol < reltol)
            || right.is_some_and(|r| (p2 - r).abs() / r.abs() + abstol / reltol < reltol)
        {
            return false;
        }
        self.trials[new].s.re = if self.trapped != 2 && t.s.re - p1 > p2 - t.s.re {
            p1
        } else {
            p2
        };
        true
    }

    /// `NIpzSym2`: deflated quadratic fit through three real trials.
    fn sym2(&mut self, set: &Set) -> SpiceResult<Real> {
        let t = |i: Option<usize>| self.trials[i.unwrap_or_default()].clone();
        let (t0, t1, t2) = (t(set[0]), t(set[1]), t(set[2]));
        let mut dx0 = t1.s.re - t0.s.re;
        let mut dx1 = t2.s.re - t1.s.re;
        let x0 = (t0.s.re + t1.s.re) / 2.0;
        let mut d2x = (t2.s.re - t0.s.re) / 2.0;

        let (mut a, mut a_mag) = zaddeq(t1.f_def.re, t1.mag_def, -t0.f_def.re, t0.mag_def);
        let mut tmag = 0;
        r_norm(&mut dx0, &mut tmag);
        a /= dx0;
        a_mag -= tmag;
        r_norm(&mut a, &mut a_mag);

        let (mut b, mut b_mag) = zaddeq(t2.f_def.re, t2.mag_def, -t1.f_def.re, t1.mag_def);
        tmag = 0;
        r_norm(&mut dx1, &mut tmag);
        b /= dx1;
        b_mag -= tmag;
        r_norm(&mut b, &mut b_mag);

        let (mut c, mut c_mag) = zaddeq(b, b_mag, -a, a_mag);
        tmag = 0;
        r_norm(&mut d2x, &mut tmag);
        c /= d2x;
        c_mag -= tmag;
        r_norm(&mut c, &mut c_mag);

        if c == 0.0 || ((a == 0.0 || c_mag < a_mag - 40) && (b == 0.0 || c_mag < b_mag - 40)) {
            if a == 0.0 {
                a = b;
                a_mag = b_mag;
            }
            if a == 0.0 {
                return Ok(t1.s.re);
            }
            let mut s = -t1.f_def.re / a;
            a_mag -= t1.mag_def;
            while a_mag > 0 {
                s /= 2.0;
                a_mag -= 1;
            }
            while a_mag < 0 {
                s *= 2.0;
                a_mag += 1;
            }
            return Ok(s + t1.s.re);
        }

        a /= c;
        r_norm(&mut a, &mut a_mag);
        a_mag -= c_mag;
        let mut diff = t1.s.re - x0;
        tmag = 0;
        r_norm(&mut diff, &mut tmag);
        (a, a_mag) = zaddeq(a, a_mag, diff, tmag);

        b = 2.0 * t1.f_def.re / c;
        b_mag = t1.mag_def - c_mag;
        r_norm(&mut b, &mut b_mag);

        let (mut disc, mut disc_mag) = zaddeq(a * a, 2 * a_mag, -b, b_mag);
        let mut complex = false;
        if disc < 0.0 {
            disc *= -1.0;
            complex = true;
        }
        if disc_mag % 2 == 0 {
            disc = disc.sqrt();
        } else {
            disc = (2.0 * disc).sqrt();
            disc_mag -= 1;
        }
        disc_mag /= 2;
        if complex {
            if self.pz_k == 0.0 {
                self.pz_k = disc;
                self.pz_k_mag = disc_mag;
            }
            disc = 0.0;
            disc_mag = 0;
        }
        (c, c_mag) = if a * disc >= 0.0 {
            zaddeq(a, a_mag, disc, disc_mag)
        } else {
            zaddeq(a, a_mag, -disc, disc_mag)
        };
        if c != 0.0 {
            b /= c;
            b_mag -= c_mag;
        } else {
            b = 0.0;
            b_mag = 0;
        }
        (b, b_mag) = zaddeq(t1.s.re, 0, -b, b_mag);
        (c, c_mag) = zaddeq(t1.s.re, 0, -c, c_mag);
        while b_mag > 0 {
            b *= 2.0;
            b_mag -= 1;
        }
        while b_mag < 0 {
            b /= 2.0;
            b_mag += 1;
        }
        while c_mag > 0 {
            c *= 2.0;
            c_mag -= 1;
        }
        while c_mag < 0 {
            c /= 2.0;
            c_mag += 1;
        }
        let outside = |x: Real| x < t0.s.re || x > t2.s.re;
        Ok(if outside(b) {
            if outside(c) {
                match self.trapped {
                    1 => (t0.s.re + t1.s.re) / 2.0,
                    2 => (t1.s.re + t2.s.re) / 2.0,
                    3 => {
                        if (t1.s.re - c).abs() < (t1.s.re - b).abs() {
                            (t1.s.re + c) / 2.0
                        } else {
                            (t1.s.re + b) / 2.0
                        }
                    }
                    _ => {
                        return Err(numerical("pole-zero search", "Lost numerical stability"));
                    }
                }
            } else {
                c
            }
        } else if outside(c) || a > 0.0 {
            b
        } else {
            c
        })
    }

    /// `NIpzMuller`.
    fn muller(&self, set: &Set) -> Cx {
        let t: [&Trial; 3] = std::array::from_fn(|i| &self.trials[set[i].unwrap_or_default()]);
        let mut min = -999_999;
        let mut count = 0;
        let mut total = 0;
        for trial in t {
            if !trial.f_def.is_zero() {
                if min < trial.mag_def - 50 {
                    min = trial.mag_def - 50;
                }
                total += trial.mag_def;
                count += 1;
            }
        }
        let mut magx = if count != 0 { total / count } else { total };
        if magx < min {
            magx = min;
        }
        let mut scale = [1.0; 3];
        for (i, trial) in t.iter().enumerate() {
            let mut mag = trial.mag_def - magx;
            while mag > 0 {
                scale[i] *= 2.0;
                mag -= 1;
            }
            if mag < -90 {
                scale[i] = 0.0;
            } else {
                while mag < 0 {
                    scale[i] /= 2.0;
                    mag += 1;
                }
            }
        }
        let f: [Cx; 3] = std::array::from_fn(|i| t[i].f_def.scale(scale[i]));
        let h0 = t[0].s.sub(t[1].s);
        let h1 = t[1].s.sub(t[2].s);
        let lambda = h0.div(h1);
        let mut delta = lambda;
        delta.re += 1.0;

        let mut a = f[2].mul(lambda);
        let mut c = f[1].mul(delta);
        a = a.sub(c);
        a = a.add(f[0]);
        a = a.mul(lambda);

        let mut b = lambda.mul(lambda);
        b = b.mul(f[2]);
        c = delta.mul(delta);
        c = c.mul(f[1]);
        b = b.sub(c);
        c = lambda.add(delta);
        c = c.mul(f[0]);
        b = b.add(c);

        c = delta.mul(f[0]);

        while a.re.abs() > 1.0
            || a.im.abs() > 1.0
            || b.re.abs() > 1.0
            || b.im.abs() > 1.0
            || c.re.abs() > 1.0
            || c.im.abs() > 1.0
        {
            a = a.scale(0.5);
            b = b.scale(0.5);
            c = c.scale(0.5);
        }
        let mut d = b.mul(b);
        let e = a.scale(4.0).mul(c);
        d = d.sub(e);
        d = d.sqrt();
        let q = b.re * d.re + b.im * d.im;
        b = if q > 0.0 { b.add(d) } else { b.sub(d) };
        c = c.div(b.scale(-0.5));
        h0.mul(c).add(t[0].s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matrix(rows: &[&[Real]]) -> Matrix {
        let mut m = Matrix::zeros(rows.len(), rows.len());
        for (r, row) in rows.iter().enumerate() {
            for (c, value) in row.iter().enumerate() {
                m.set(r, c, *value).unwrap();
            }
        }
        m
    }

    fn close(found: &[Complex], expected: &[Complex]) {
        assert_eq!(found.len(), expected.len(), "{found:?}");
        for (f, e) in found.iter().zip(expected) {
            assert!(
                (f.re - e.re).abs() <= 1e-6 * e.re.abs().max(1.0)
                    && (f.im - e.im).abs() <= 1e-6 * e.im.abs().max(1.0),
                "{found:?} vs {expected:?}"
            );
        }
    }

    #[test]
    fn real_roots_of_an_rc_ladder() {
        // det(A + sE) = (1 + s)(2 + s) for a diagonal pencil.
        let a = matrix(&[&[1.0, 0.0], &[0.0, 2.0]]);
        let e = matrix(&[&[1.0, 0.0], &[0.0, 1.0]]);
        let roots = pencil_roots(&a, &e).unwrap().roots;
        close(&roots, &[Complex::new(-2.0, 0.0), Complex::new(-1.0, 0.0)]);
    }

    #[test]
    fn complex_pair_of_an_lc_tank() {
        // s^2 + 0.2 s + 1: roots -0.1 +- j sqrt(0.99).
        let a = matrix(&[&[0.2, 1.0], &[-1.0, 0.0]]);
        let e = matrix(&[&[1.0, 0.0], &[0.0, 1.0]]);
        let roots = pencil_roots(&a, &e).unwrap().roots;
        let w = 0.99_f64.sqrt();
        close(&roots, &[Complex::new(-0.1, w), Complex::new(-0.1, -w)]);
    }

    #[test]
    fn a_pencil_without_finite_roots_has_none() {
        let a = matrix(&[&[1.0, 0.0], &[0.0, 1.0]]);
        let e = matrix(&[&[0.0, 0.0], &[0.0, 0.0]]);
        assert!(pencil_roots(&a, &e).unwrap().roots.is_empty());
    }

    #[test]
    fn a_determinant_vanishing_everywhere_is_c_e_short() {
        let a = matrix(&[&[1.0, 1.0], &[1.0, 1.0]]);
        let e = matrix(&[&[1.0, 1.0], &[1.0, 1.0]]);
        let error = pencil_roots(&a, &e).unwrap_err().to_string();
        assert!(error.contains("singular pencil"), "{error}");
    }
}
