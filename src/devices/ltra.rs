//! Lossy transmission line `O` with an `ltra` model (C
//! `src/spicelib/devices/ltra/`, parser `src/spicelib/parser/inp2o.c`).
//!
//! ```text
//! Oname p1 n1 p2 n2 model [v1= i1= v2= i2=] [ic=v1[,i1[,v2[,i2]]]]
//! .model model ltra [r=] [l=] [g=] [c=] len= [rel=] [abs=] [compactrel=]
//!        [compactabs=] [steplimit|nosteplimit] [lininterp|quadinterp|mixedinterp]
//!        [truncdontcut] [truncnr] [nocontrol] [ltra]
//! ```
//!
//! # Cases (`ltraset.c`)
//!
//! C classifies the per-metre `R L G C` into special cases and simulates only
//! four of them; the port does the same, with C's errors for the others:
//!
//! | Case | Condition | Model |
//! | --- | --- | --- |
//! | LC | `R = G = 0`, `L, C > 0` | lossless line: delayed waves, attenuation 1 |
//! | RLC | `R > 0`, `G = 0`, `L, C > 0` | delayed waves times `exp(-beta TD)` plus convolutions with `h1'`, `h2`, `h3'` (Bessel-function impulse responses) |
//! | RC | `R > 0`, `G = L = 0`, `C > 0` | diffusion line: convolutions with the `erfc` responses, no delay |
//! | RG | `R > 0`, `G > 0`, `L = C = 0` | resistive line: the exact DC two-port at every analysis |
//! | RL | `R > 0`, `G = C = 0`, `L > 0` | error ("RL line not supported yet") |
//! | `G > 0` with `L` or `C` | | error ("Nonzero G (except RG) line not supported yet") |
//! | fewer than two nonzero | | error |
//!
//! # Unknowns and signs
//!
//! `ltraset.c` creates two current unknowns with `CKTmkVolt`, so they are
//! node unknowns named `<name>#i1` and `<name>#i2` (plots carry `v(o1#i1)`,
//! amperes typed `voltage`, as ngspice writes them). `i1`/`i2` are the
//! currents **into** the positive terminal of port 1/2. There are no internal
//! voltage nodes. With `Y0 = 1/Z0`:
//!
//! | Analysis | Equations | C |
//! | --- | --- | --- |
//! | DC (LC, RLC, RC) | `i1 + i2 = 0`, `v(p1) - v(p2) = R len i1` (C's DC load ignores the negative terminals) | `ltraload.c`, `MODEDC` |
//! | DC, transient (RG; AC refused, below) | `v1 = cosh(l sqrt(RG)) v2 - (1 + gmin) sqrt(R/G) sinh(.) i2`, `cosh(.) i2 - (1 + gmin) sqrt(G/R) sinh(.) v2 + i1 = 0` (as stamped by C) | `ltraload.c` |
//! | AC, `.sp` | `Y0 V1 - I1 = exp(-lambda len) (Y0 V2 + I2)` and symmetrically, `Y0`, `lambda` exact at each frequency | `ltraacld.c` |
//! | transient LC/RLC | `Y0 v1 - i1 (+ Y0 h1'(0) v1) = input1` | `ltraload.c` |
//! | transient RC | `-i1 + h1'(0) v1 - h2(0) i2 - h3'(0) v2 = input1` | `ltraload.c` |
//!
//! The AC coefficients are complex and frequency dependent; the line puts
//! their real part into `A` and their imaginary part divided by `omega` into
//! `E` at every frequency ([`Device::small_signal_depends_on_frequency`]), as
//! the lossless line does. At frequency zero it stamps its DC form.
//!
//! # Transient (`ltraload.c`, `ltraacct.c`, `ltratrun.c`, `ltramisc.c`)
//!
//! * **History.** C keeps every accepted time (`CKTtimePoints`) and the
//!   port voltages and currents there (`LTRAv1`, `LTRAi1`, `LTRAv2`,
//!   `LTRAi2`), never pruned: the convolutions run over the whole past. The
//!   port keeps the same samples in the device's [`DelayHistory`] (width
//!   4: `v1 i1 v2 i2`), starting with the operating point at `t = 0` and
//!   appending every accepted point; trials only read it.
//! * **Load.** At each trial time the line computes C's convolution
//!   coefficients ([`convolution::rlc_coefficients`],
//!   [`convolution::rc_coefficients`]) over the accepted times, stamps the
//!   coefficients of the unknown sample into the matrix and puts the
//!   convolution of the history (minus the operating-point initial
//!   conditions) and the delayed waves at `t - TD` (quadratic, linear or
//!   mixed interpolation, `LTRAquadInterp`/`LTRAlinInterp`) into the
//!   right-hand side.
//! * **Breakpoints.** On acceptance the line requests a breakpoint at the
//!   previous accepted time plus `TD` when either attenuated wave
//!   `(v + Z0 i) exp(-beta TD)` changes slope by at least `rel max(|d1|,
//!   |d2|) + abs` and is not steady (C's `CHECK` with `50 reltol`,
//!   `50 abstol`). A request in the past is an error, as `CKTsetBreak`
//!   ("breakpoint in the past") makes it in C.
//! * **Step bound.** LC/RLC lines bound every step by `TD` (`steplimit`, the
//!   default) or, under `nosteplimit`, when the slope test fires for the
//!   trial; RLC lines also by `LTRAmaxSafeStep` unless `truncdontcut`. C's
//!   truncation-error control (`LTRAlteCalculate`) is unreachable from a
//!   deck (`LTRAlteConType` can only be `nocontrol`), and so is `truncnr`.
//!
//! # Deliberate divergences (explicit errors)
//!
//! * Negative `R`, `L`, `G`, `C`, a nonpositive or missing `len`, negative
//!   `rel`/`abs`/`compactrel`/`compactabs` (C computes square roots of
//!   negative numbers or a zero delay).
//! * A `level` or any setter outside `LTRAmPTable` on the model (C warns and
//!   ignores), instance-parameter defaults on the model card, a leading
//!   value on the instance (C ignores it).
//! * `.tran ... uic`, `.options trytocompact` (refused by the options
//!   layer), `.pz`, `.noise`, `.disto`, `.sens`, the diffsol BDF backend.
//! * RG lines in AC/`.sp`: C's `LTRAacLoad` loads the DC stamp of every LTRA
//!   model and returns at the first RG model, corrupting any other lossy
//!   line's AC load; the device cannot see the other lines, so it refuses.
//! * A nonfinite convolution input, which C's Bessel approximations produce
//!   once `beta t` exceeds about 700 (`exp` overflow); C continues with NaN.
//! * C reports `@o1[v1]` and the other initial conditions after a transient
//!   as the operating point it copied into them (`ltraload.c`
//!   `MODEINITTRAN`); the port offers no instance observations.

pub mod convolution;

use crate::devices::delay::{DelayContext, DelayHistory, DelayLine, DelayUpdate};
use crate::devices::linear::LinearContext;
use crate::devices::models::{ModelContext, ModelFamily, ModelResolver};
use crate::devices::schema::{ScalarDomain, ScalarParameter, ScalarSchema, ScalarUnit};
use crate::devices::traits::{AnalysisMode, Device, StampContext};
use crate::maths::Vector;
use crate::netlist::ast::{DeviceInstance, ParameterKind};
use crate::primitives::{
    NodeId, NodeKind, NodeTable, Real, SourceLoc, SpiceError, SpiceResult, parse_spice_number,
};

/// C reference for errors.
const C_REFERENCE: &str = "src/spicelib/devices/ltra/";

/// Internal-node suffixes in `ltraset.c`'s creation order (`CKTmkVolt`).
pub const INTERNAL_SUFFIXES: [&str; 2] = ["i1", "i2"];

/// `LTRAmPTable` scalars (`ltra.c`). `r`, `l`, `g`, `c` default to 0, `len` is
/// required, `rel`/`abs` default to 1 (`ltraset.c`), and `compactrel`/
/// `compactabs` of 0 mean C's `reltol`/`abstol`.
const MODEL: ScalarSchema<'static> = ScalarSchema {
    parameters: &[
        scalar("r", ScalarUnit::OhmPerMetre, Some(0.)),
        scalar("l", ScalarUnit::HenryPerMetre, Some(0.)),
        scalar("g", ScalarUnit::SiemensPerMetre, Some(0.)),
        scalar("c", ScalarUnit::FaradPerMetre, Some(0.)),
        ScalarParameter {
            name: "len",
            unit: ScalarUnit::Metre,
            domain: ScalarDomain::Positive,
            default: None,
        },
        scalar("rel", ScalarUnit::Dimensionless, Some(1.)),
        scalar("abs", ScalarUnit::Dimensionless, Some(1.)),
        scalar("compactrel", ScalarUnit::Dimensionless, Some(0.)),
        scalar("compactabs", ScalarUnit::Dimensionless, Some(0.)),
    ],
};

const fn scalar(name: &'static str, unit: ScalarUnit, default: Option<Real>) -> ScalarParameter {
    ScalarParameter {
        name,
        unit,
        domain: ScalarDomain::NonNegative,
        default,
    }
}

/// C's `LTRAspecialCase` of a simulated line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LtraCase {
    /// Lossless (`R = G = 0`).
    Lc,
    /// Series loss only (`G = 0`).
    Rlc,
    /// Distributed RC (`L = G = 0`).
    Rc,
    /// Distributed RG (`L = C = 0`).
    Rg,
}

/// `LTRAhowToInterp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    /// `lininterp`.
    Linear,
    /// `quadinterp` (the default without `trytocompact`).
    Quadratic,
    /// `mixedinterp`: quadratic unless it leaves the range of the samples.
    Mixed,
}

/// The validated `ltra` model with `ltratemp.c`'s derived quantities.
#[derive(Debug, Clone, PartialEq)]
pub struct LtraModel {
    name: String,
    case: LtraCase,
    resist: Real,
    induct: Real,
    conduct: Real,
    capac: Real,
    length: Real,
    reltol: Real,
    abstol: Real,
    st_line_reltol: Real,
    st_line_abstol: Real,
    step_limit: bool,
    interpolation: Interpolation,
    trunc_dont_cut: bool,
    // ltratemp.c
    td: Real,
    imped: Real,
    admit: Real,
    alpha: Real,
    beta: Real,
    attenuation: Real,
    cbyr: Real,
    rclsqr: Real,
    int_h1dash: Real,
    int_h2: Real,
    int_h3dash: Real,
    // ltraload.c, MODEDC, RG case
    cosh_root_gr: Real,
    r_rs_lrgr_or_g: Real,
    r_gs_lrgr_or_r: Real,
}

impl LtraModel {
    /// Reads, validates and classifies an `ltra` model card (`LTRAmParam`,
    /// `ltraset.c`, `ltratemp.c`).
    ///
    /// # Errors
    /// Unknown setters, `level`, invalid values, a missing `len` and C's
    /// unsupported cases.
    pub fn from_card(card: &crate::netlist::ast::ModelCard) -> SpiceResult<Self> {
        let mut setters = Vec::with_capacity(card.parameters.len());
        let mut step_limit = true;
        let mut interpolation = Interpolation::Quadratic;
        let mut trunc_dont_cut = false;
        for parameter in &card.parameters {
            if parameter.kind == ParameterKind::Flag {
                match parameter.name.as_str() {
                    // LTRA_MOD_LTRA, the (default) LTRA_MOD_NOCONTROL and
                    // LTRA_MOD_TRUNCNR (read only by the unreachable
                    // truncation-error control) change nothing.
                    "ltra" | "nocontrol" | "truncnr" => {}
                    "steplimit" => step_limit = true,
                    "nosteplimit" => step_limit = false,
                    "lininterp" => interpolation = Interpolation::Linear,
                    "quadinterp" => interpolation = Interpolation::Quadratic,
                    "mixedinterp" => interpolation = Interpolation::Mixed,
                    "truncdontcut" => trunc_dont_cut = true,
                    other => {
                        return Err(SpiceError::parse(
                            parameter.location.clone(),
                            format!("unknown ltra model flag '{other}' (ltra.c LTRAmPTable)"),
                        ));
                    }
                }
                continue;
            }
            if parameter.name.eq_ignore_ascii_case("level") {
                return Err(SpiceError::Unsupported {
                    feature: "level on an ltra model (LTRAmPTable has no level; C ignores it)"
                        .into(),
                    location: Some(parameter.location.clone()),
                });
            }
            if matches!(parameter.name.as_str(), "v1" | "v2" | "i1" | "i2" | "ic") {
                return Err(SpiceError::Unsupported {
                    feature: format!(
                        "instance-parameter default '{}' on an ltra model (inpgmod.c defaults)",
                        parameter.name
                    ),
                    location: Some(parameter.location.clone()),
                });
            }
            setters.push(parameter);
        }
        let values = MODEL.validate(setters, &card.location)?;
        let get = |name: &str| values.get(name).map_or(0., |v| v.value);
        let Some(length) = values.get("len").map(|v| v.value) else {
            return Err(SpiceError::parse(
                card.location.clone(),
                format!("{}: lossy line length must be given (ltraset.c)", card.name),
            ));
        };
        let (resist, induct, conduct, capac) = (get("r"), get("l"), get("g"), get("c"));
        let fail = |message: &str| {
            Err(SpiceError::parse(
                card.location.clone(),
                format!("{}: {message} (ltraset.c)", card.name),
            ))
        };
        // ltraset.c's classification, in C's order (later tests win).
        let mut case = None;
        if resist == 0. && conduct == 0. && capac != 0. && induct != 0. {
            case = Some(LtraCase::Lc);
        }
        if resist != 0. && conduct == 0. && capac != 0. && induct != 0. {
            case = Some(LtraCase::Rlc);
        }
        if resist != 0. && conduct == 0. && capac != 0. && induct == 0. {
            case = Some(LtraCase::Rc);
        }
        if resist != 0. && conduct == 0. && capac == 0. && induct != 0. {
            return fail("RL line not supported yet");
        }
        if resist != 0. && conduct != 0. && capac == 0. && induct == 0. {
            case = Some(LtraCase::Rg);
        }
        if conduct != 0. && (capac != 0. || induct != 0.) {
            return fail("Nonzero G (except RG) line not supported yet");
        }
        let nonzero = [resist, conduct, induct, capac]
            .iter()
            .filter(|v| **v != 0.)
            .count();
        if nonzero <= 1 {
            return fail("At least two of R,L,G,C must be specified and nonzero");
        }
        let Some(case) = case else {
            return fail("unclassified R, L, G, C combination");
        };
        let mut model = Self {
            name: card.name.clone(),
            case,
            resist,
            induct,
            conduct,
            capac,
            length,
            reltol: get("rel"),
            abstol: get("abs"),
            st_line_reltol: get("compactrel"),
            st_line_abstol: get("compactabs"),
            step_limit,
            interpolation,
            trunc_dont_cut,
            td: 0.,
            imped: 0.,
            admit: 0.,
            alpha: 0.,
            beta: 0.,
            attenuation: 0.,
            cbyr: 0.,
            rclsqr: 0.,
            int_h1dash: 0.,
            int_h2: 0.,
            int_h3dash: 0.,
            cosh_root_gr: 0.,
            r_rs_lrgr_or_g: 0.,
            r_gs_lrgr_or_r: 0.,
        };
        model.temperature();
        let derived = [
            model.td,
            model.imped,
            model.admit,
            model.attenuation,
            model.cbyr,
            model.rclsqr,
            model.cosh_root_gr,
            model.r_rs_lrgr_or_g,
            model.r_gs_lrgr_or_r,
        ];
        if derived.iter().any(|v| !v.is_finite()) {
            return fail("derived line constants are not finite");
        }
        Ok(model)
    }

    /// `ltratemp.c` (and the RG constants of `ltraload.c`'s DC branch).
    fn temperature(&mut self) {
        match self.case {
            LtraCase::Lc => {
                self.imped = (self.induct / self.capac).sqrt();
                self.admit = 1. / self.imped;
                self.td = (self.induct * self.capac).sqrt() * self.length;
                self.attenuation = 1.0;
            }
            LtraCase::Rlc => {
                self.imped = (self.induct / self.capac).sqrt();
                self.admit = 1. / self.imped;
                self.td = (self.induct * self.capac).sqrt() * self.length;
                self.alpha = 0.5 * (self.resist / self.induct);
                self.beta = self.alpha;
                self.attenuation = (-self.beta * self.td).exp();
                // alpha > 0 (R, L > 0).
                self.int_h1dash = -1.0;
                self.int_h2 = 1.0 - self.attenuation;
                self.int_h3dash = -self.attenuation;
            }
            LtraCase::Rc => {
                self.cbyr = self.capac / self.resist;
                self.rclsqr = self.resist * self.capac * self.length * self.length;
                self.int_h1dash = 0.0;
                self.int_h2 = 1.0;
                self.int_h3dash = 0.0;
            }
            LtraCase::Rg => {
                let mut dummy1 = self.length * (self.resist * self.conduct).sqrt();
                let dummy2 = (-dummy1).exp();
                dummy1 = dummy1.exp();
                self.cosh_root_gr = 0.5 * (dummy1 + dummy2);
                self.r_rs_lrgr_or_g = if self.conduct <= 1.0e-10 {
                    self.length * self.resist
                } else {
                    0.5 * (dummy1 - dummy2) * (self.resist / self.conduct).sqrt()
                };
                self.r_gs_lrgr_or_r = if self.resist <= 1.0e-10 {
                    self.length * self.conduct
                } else {
                    0.5 * (dummy1 - dummy2) * (self.conduct / self.resist).sqrt()
                };
            }
        }
    }

    /// The simulated case.
    #[must_use]
    pub const fn case(&self) -> LtraCase {
        self.case
    }

    /// Propagation delay `sqrt(L C) len` (LC and RLC; 0 otherwise).
    #[must_use]
    pub const fn td(&self) -> Real {
        self.td
    }

    /// Characteristic impedance `sqrt(L / C)` (LC and RLC; 0 otherwise).
    #[must_use]
    pub const fn z0(&self) -> Real {
        self.imped
    }
}

/// A lossy transmission line instance.
#[derive(Debug, Clone, PartialEq)]
pub struct LossyLine {
    name: String,
    /// `p1 n1 p2 n2`, then the current unknowns `i1 i2`.
    terminals: [NodeId; 6],
    model: LtraModel,
    /// `v1 i1 v2 i2` (used by C only under `uic`).
    initial: [Real; 4],
    location: SourceLoc,
}

fn invalid(location: &SourceLoc, message: impl Into<String>) -> SpiceError {
    SpiceError::Parse {
        location: location.clone(),
        message: message.into(),
    }
}

/// The convolution and interpolation state of one transient load.
struct Load {
    /// Matrix coefficients of the unknown sample (`h1'`, `h2`, `h3'`).
    first: [Real; 3],
    /// `LTRAinput1`, `LTRAinput2`.
    inputs: [Real; 2],
}

impl LossyLine {
    /// Builds the line from its instance card and `ltra` model, interning
    /// the four terminals and the two current unknowns (nothing is interned
    /// on error).
    ///
    /// # Errors
    /// A missing or wrong model, invalid model or instance values, unknown
    /// setters or node-name collisions.
    pub fn instantiate(
        instance: &DeviceInstance,
        nodes: &mut NodeTable,
        models: &ModelResolver<'_>,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        context.validate(&instance.location)?;
        if instance.nodes.len() != 4 {
            return Err(SpiceError::circuit(format!(
                "{}: a lossy transmission line has four terminals",
                instance.name
            )));
        }
        let Some(model) = models.resolve(instance)? else {
            return Err(invalid(
                &instance.location,
                format!(
                    "{}: a lossy line requires an ltra model (inp2o.c)",
                    instance.name
                ),
            ));
        };
        if model.family() != ModelFamily::Ltra {
            return Err(invalid(
                &instance.location,
                format!("{}: incorrect model type (inp2o.c)", instance.name),
            ));
        }
        let model = LtraModel::from_card(model.card())?;
        let mut initial = [0.; 4];
        let number = |text: &str, at: &SourceLoc, name: &str| {
            parse_spice_number(text)
                .filter(|v| v.is_finite())
                .ok_or_else(|| invalid(at, format!("{name} must be a finite number, not '{text}'")))
        };
        for p in &instance.parameters {
            match (&p.kind, p.name.as_str()) {
                (ParameterKind::InitialConditions(values), "ic") => {
                    for value in values {
                        let slot = match value.name.as_str() {
                            "v1" => 0,
                            "i1" => 1,
                            "v2" => 2,
                            "i2" => 3,
                            other => {
                                return Err(invalid(
                                    &value.value.location,
                                    format!("unexpected ic component {other}"),
                                ));
                            }
                        };
                        initial[slot] = number(&value.value.text, &value.value.location, "ic")?;
                    }
                }
                (ParameterKind::Scalar, name @ ("v1" | "i1" | "v2" | "i2")) => {
                    let slot = match name {
                        "v1" => 0,
                        "i1" => 1,
                        "v2" => 2,
                        _ => 3,
                    };
                    initial[slot] = number(&p.value, &p.location, name)?;
                }
                (ParameterKind::Scalar, other) => {
                    return Err(invalid(
                        &p.location,
                        format!("unknown lossy-line parameter {other} (LTRApTable)"),
                    ));
                }
                _ => {
                    return Err(SpiceError::not_yet_ported(
                        format!(
                            "{}: lossy-line setter '{}' of this form",
                            p.location, p.name
                        ),
                        "src/spicelib/devices/ltra/ltrapar.c",
                    ));
                }
            }
        }
        let mut staged = nodes.clone();
        let mut terminals = [NodeId::GROUND; 6];
        for (slot, node) in instance.nodes.iter().enumerate() {
            terminals[slot] = staged.intern(node);
        }
        for (slot, suffix) in INTERNAL_SUFFIXES.iter().enumerate() {
            let name = format!("{}#{suffix}", instance.name);
            if staged.get(&name).is_some() {
                return Err(SpiceError::circuit(format!(
                    "lossy-line internal-node name collision: {name}"
                )));
            }
            let node = staged.intern(&name);
            staged.set_kind(node, NodeKind::Internal);
            terminals[4 + slot] = node;
        }
        *nodes = staged;
        Ok(Box::new(Self {
            name: instance.name.clone(),
            terminals,
            model,
            initial,
            location: instance.location.clone(),
        }))
    }

    /// The validated model.
    #[must_use]
    pub const fn model(&self) -> &LtraModel {
        &self.model
    }

    /// The instance `v1 i1 v2 i2` initial conditions.
    #[must_use]
    pub const fn initial_conditions(&self) -> [Real; 4] {
        self.initial
    }

    const fn pos1(&self) -> NodeId {
        self.terminals[0]
    }
    const fn neg1(&self) -> NodeId {
        self.terminals[1]
    }
    const fn pos2(&self) -> NodeId {
        self.terminals[2]
    }
    const fn neg2(&self) -> NodeId {
        self.terminals[3]
    }
    const fn ibr1(&self) -> NodeId {
        self.terminals[4]
    }
    const fn ibr2(&self) -> NodeId {
        self.terminals[5]
    }

    /// The DC entries (`ltraload.c`, `MODEDC`) with C's `gmin`.
    fn dc_entries(&self, gmin: Real) -> Vec<(NodeId, NodeId, Real)> {
        let m = &self.model;
        let (p1, n1, p2, n2, b1, b2) = (
            self.pos1(),
            self.neg1(),
            self.pos2(),
            self.neg2(),
            self.ibr1(),
            self.ibr2(),
        );
        match m.case {
            LtraCase::Rg => vec![
                (b1, p1, 1.0),
                (b1, n1, -1.0),
                (b1, p2, -m.cosh_root_gr),
                (b1, n2, m.cosh_root_gr),
                (b1, b2, (1. + gmin) * m.r_rs_lrgr_or_g),
                (b2, b2, m.cosh_root_gr),
                (b2, p2, -(1. + gmin) * m.r_gs_lrgr_or_r),
                (b2, n2, (1. + gmin) * m.r_gs_lrgr_or_r),
                (b2, b1, 1.0),
                (p1, b1, 1.0),
                (n1, b1, -1.0),
                (p2, b2, 1.0),
                (n2, b2, -1.0),
            ],
            LtraCase::Lc | LtraCase::Rlc | LtraCase::Rc => vec![
                (p1, b1, 1.0),
                (n1, b1, -1.0),
                (p2, b2, 1.0),
                (n2, b2, -1.0),
                (b1, b1, 1.0),
                (b1, b2, 1.0),
                (b2, p1, 1.0),
                (b2, p2, -1.0),
                (b2, b1, -m.resist * m.length),
            ],
        }
    }

    /// The transient matrix entries (`ltraload.c`, all non-DC cases but RG)
    /// given the first convolution coefficients.
    fn transient_entries(&self, first: [Real; 3]) -> Vec<(NodeId, NodeId, Real)> {
        let m = &self.model;
        let (p1, n1, p2, n2, b1, b2) = (
            self.pos1(),
            self.neg1(),
            self.pos2(),
            self.neg2(),
            self.ibr1(),
            self.ibr2(),
        );
        let mut entries = Vec::with_capacity(20);
        match m.case {
            LtraCase::Lc | LtraCase::Rlc => {
                if m.case == LtraCase::Rlc {
                    let d = m.admit * first[0];
                    entries.extend([(b1, p1, d), (b1, n1, -d), (b2, p2, d), (b2, n2, -d)]);
                }
                entries.extend([
                    (b1, p1, m.admit),
                    (b1, n1, -m.admit),
                    (b1, b1, -1.0),
                    (p1, b1, 1.0),
                    (n1, b1, -1.0),
                    (b2, p2, m.admit),
                    (b2, n2, -m.admit),
                    (b2, b2, -1.0),
                    (p2, b2, 1.0),
                    (n2, b2, -1.0),
                ]);
            }
            LtraCase::Rc => {
                let [h1, h2, h3] = first;
                entries.extend([
                    (b1, b1, -1.0),
                    (p1, b1, 1.0),
                    (n1, b1, -1.0),
                    (b2, b2, -1.0),
                    (p2, b2, 1.0),
                    (n2, b2, -1.0),
                    (b1, p1, h1),
                    (b1, n1, -h1),
                    (b2, p2, h1),
                    (b2, n2, -h1),
                    (b1, b2, -h2),
                    (b2, b1, -h2),
                    (b1, p2, -h3),
                    (b1, n2, h3),
                    (b2, p1, -h3),
                    (b2, n1, h3),
                ]);
            }
            LtraCase::Rg => {}
        }
        entries
    }

    /// `[v1, i1, v2, i2]` of a solution: the port voltages and the current
    /// unknowns (C reads them from `CKTrhsOld`).
    fn port_values(&self, context: &DelayContext<'_>) -> [Real; 4] {
        let v = |node| context.node_voltage(node);
        [
            v(self.pos1()) - v(self.neg1()),
            v(self.ibr1()),
            v(self.pos2()) - v(self.neg2()),
            v(self.ibr2()),
        ]
    }

    fn internal(&self, message: impl Into<String>) -> SpiceError {
        SpiceError::Numerical {
            context: self.name.clone(),
            message: message.into(),
        }
    }

    /// `ltraload.c` for a transient load at `time` (C's `MODEINITPRED`
    /// computation of the coefficients and `LTRAinput1/2`, which C then keeps
    /// for the remaining iterations of the time point).
    #[allow(clippy::too_many_lines)]
    fn transient_load(&self, history: &DelayHistory, time: Real) -> SpiceResult<Load> {
        let m = &self.model;
        let count = history.len();
        if count == 0 {
            return Err(SpiceError::circuit(format!(
                "{}: delay history used before the transient start",
                self.name
            )));
        }
        let index = count - 1;
        let times: Vec<Real> = (0..count)
            .map(|i| history.time(i).unwrap_or(Real::NAN))
            .collect();
        let sample = |i: usize, k: usize| history.value(i, k).unwrap_or(Real::NAN);
        // The operating point C copies into LTRAinitVolt/Cur at MODEINITTRAN
        // is the t = 0 sample.
        let [iv1, ii1, iv2, ii2] = [0, 1, 2, 3].map(|k| sample(0, k));
        let tdover = matches!(m.case, LtraCase::Lc | LtraCase::Rlc) && time > m.td;
        let reltol_chop = 0.; // LTRAchopReltol: no deck setter.
        let coefficients = match m.case {
            LtraCase::Rlc => {
                convolution::rlc_coefficients(m.td, m.alpha, m.beta, time, &times, reltol_chop)
            }
            LtraCase::Rc => {
                convolution::rc_coefficients(m.cbyr, m.rclsqr, time, &times, reltol_chop)
            }
            LtraCase::Lc | LtraCase::Rg => convolution::Coefficients::default(),
        };
        // The delayed values v1d i1d v2d i2d.
        let mut delayed = [0.; 4];
        if tdover {
            let target = time - m.td;
            let mut found = None;
            for i in (0..=index).rev() {
                if times[i] < target {
                    found = Some(i);
                    break;
                }
            }
            let mut i = found.ok_or_else(|| {
                self.internal("cannot find the delayed time point (ltraload.c E_INTERN)")
            })?;
            if i == index {
                // Step larger than the delay: extrapolate from the last two.
                i = i.checked_sub(1).ok_or_else(|| {
                    self.internal(
                        "the first time step exceeds the line delay (ltraload.c E_INTERN)",
                    )
                })?;
            }
            let (t2, t3) = (times[i], times[i + 1]);
            let quadratic = matches!(
                m.interpolation,
                Interpolation::Quadratic | Interpolation::Mixed
            );
            let qf = if i != 0 && quadratic {
                Some(
                    convolution::quad_interp(target, times[i - 1], t2, t3)
                        .ok_or_else(|| self.internal("coincident history times"))?,
                )
            } else {
                None
            };
            let lf = if i == 0
                || matches!(
                    m.interpolation,
                    Interpolation::Mixed | Interpolation::Linear
                ) {
                Some(
                    convolution::lin_interp(target, t2, t3)
                        .ok_or_else(|| self.internal("coincident history times"))?,
                )
            } else {
                None
            };
            for (k, value) in delayed.iter_mut().enumerate() {
                let mut d = 0.;
                let mut range = (0., 0.);
                if let Some([q1, q2, q3]) = qf {
                    let (a, b, c) = (sample(i - 1, k), sample(i, k), sample(i + 1, k));
                    d = a * q1 + b * q2 + c * q3;
                    range = (a.max(b).max(c), a.min(b).min(c));
                }
                let out_of_range = qf.is_some() && (d > range.0 || d < range.1);
                // C falls back to linear for lininterp, at the first sample,
                // and for mixedinterp when the quadratic leaves the samples'
                // range (quadinterp keeps the out-of-range value).
                let linear = m.interpolation == Interpolation::Linear || i == 0 || out_of_range;
                if linear && !(i != 0 && m.interpolation == Interpolation::Quadratic) {
                    let [l2, l3] =
                        lf.ok_or_else(|| self.internal("missing linear interpolation"))?;
                    d = sample(i, k) * l2 + sample(i + 1, k) * l3;
                }
                *value = d;
            }
        }
        let [v1d, i1d, v2d, i2d] = delayed;
        let mut inputs = [0.; 2];
        match m.case {
            LtraCase::Rlc => {
                let c = &coefficients;
                // h1' with v1 and v2.
                let (mut d1, mut d2) = (0., 0.);
                for i in (1..=index).rev() {
                    if c.h1dash[i] != 0.0 {
                        d1 += c.h1dash[i] * (sample(i, 0) - iv1);
                        d2 += c.h1dash[i] * (sample(i, 2) - iv2);
                    }
                }
                d1 += iv1 * m.int_h1dash;
                d2 += iv2 * m.int_h1dash;
                d1 -= iv1 * c.first[0];
                d2 -= iv2 * c.first[0];
                inputs[0] -= d1 * m.admit;
                inputs[1] -= d2 * m.admit;
                // h2 with i2 and i1.
                let (mut d1, mut d2) = (0., 0.);
                if tdover {
                    d1 = (i2d - ii2) * c.first[1];
                    d2 = (i1d - ii1) * c.first[1];
                    for i in (1..=c.aux_index).rev() {
                        if c.h2[i] != 0.0 {
                            d1 += c.h2[i] * (sample(i, 3) - ii2);
                            d2 += c.h2[i] * (sample(i, 1) - ii1);
                        }
                    }
                }
                d1 += ii2 * m.int_h2;
                d2 += ii1 * m.int_h2;
                inputs[0] += d1;
                inputs[1] += d2;
                // h3' with v2 and v1.
                let (mut d1, mut d2) = (0., 0.);
                if tdover {
                    d1 = (v2d - iv2) * c.first[2];
                    d2 = (v1d - iv1) * c.first[2];
                    for i in (1..=c.aux_index).rev() {
                        if c.h3dash[i] != 0.0 {
                            d1 += c.h3dash[i] * (sample(i, 2) - iv2);
                            d2 += c.h3dash[i] * (sample(i, 0) - iv1);
                        }
                    }
                }
                d1 += iv2 * m.int_h3dash;
                d2 += iv1 * m.int_h3dash;
                inputs[0] += m.admit * d1;
                inputs[1] += m.admit * d2;
                self.lossless_inputs(&mut inputs, tdover, [iv1, ii1, iv2, ii2], delayed);
            }
            LtraCase::Lc => {
                self.lossless_inputs(&mut inputs, tdover, [iv1, ii1, iv2, ii2], delayed);
            }
            LtraCase::Rc => {
                let c = &coefficients;
                let (mut d1, mut d2) = (0., 0.);
                for i in (1..=index).rev() {
                    if c.h1dash[i] != 0.0 {
                        d1 += c.h1dash[i] * (sample(i, 0) - iv1);
                        d2 += c.h1dash[i] * (sample(i, 2) - iv2);
                    }
                }
                d1 += iv1 * m.int_h1dash;
                d2 += iv2 * m.int_h1dash;
                d1 -= iv1 * c.first[0];
                d2 -= iv2 * c.first[0];
                inputs[0] -= d1;
                inputs[1] -= d2;
                let (mut d1, mut d2) = (0., 0.);
                for i in (1..=index).rev() {
                    if c.h2[i] != 0.0 {
                        d1 += c.h2[i] * (sample(i, 3) - ii2);
                        d2 += c.h2[i] * (sample(i, 1) - ii1);
                    }
                }
                d1 += ii2 * m.int_h2;
                d2 += ii1 * m.int_h2;
                d1 -= ii2 * c.first[1];
                d2 -= ii1 * c.first[1];
                inputs[0] += d1;
                inputs[1] += d2;
                let (mut d1, mut d2) = (0., 0.);
                for i in (1..=index).rev() {
                    if c.h3dash[i] != 0.0 {
                        d1 += c.h3dash[i] * (sample(i, 2) - iv2);
                        d2 += c.h3dash[i] * (sample(i, 0) - iv1);
                    }
                }
                d1 += iv2 * m.int_h3dash;
                d2 += iv1 * m.int_h3dash;
                d1 -= iv2 * c.first[2];
                d2 -= iv1 * c.first[2];
                inputs[0] += d1;
                inputs[1] += d2;
            }
            LtraCase::Rg => {}
        }
        if !(inputs.iter().all(|v| v.is_finite())
            && coefficients.first.iter().all(|v| v.is_finite()))
        {
            return Err(self.internal(format!(
                "nonfinite convolution at t = {time:e} (ltramisc.c Bessel/erfc terms overflow once \
                 beta t exceeds about 700; C continues with NaN)"
            )));
        }
        Ok(Load {
            first: coefficients.first,
            inputs,
        })
    }

    /// The lossless-like part of `ltraload.c` (LC and the end of RLC).
    fn lossless_inputs(
        &self,
        inputs: &mut [Real; 2],
        tdover: bool,
        [iv1, ii1, iv2, ii2]: [Real; 4],
        [v1d, i1d, v2d, i2d]: [Real; 4],
    ) {
        let m = &self.model;
        if tdover {
            inputs[0] += m.attenuation * (v2d * m.admit + i2d);
            inputs[1] += m.attenuation * (v1d * m.admit + i1d);
        } else {
            inputs[0] += m.attenuation * (iv2 * m.admit + ii2);
            inputs[1] += m.attenuation * (iv1 * m.admit + ii1);
        }
    }

    /// `ltraacld.c`: the complex entries at angular frequency `omega > 0`, as
    /// `(row, column, re, im)`.
    fn ac_entries(&self, omega: Real) -> Vec<(NodeId, NodeId, Real, Real)> {
        let m = &self.model;
        let (y0_r, y0_i, lambda_r, lambda_i) = match m.case {
            LtraCase::Lc => (m.admit, 0.0, 0.0, (m.induct * m.capac).sqrt() * omega),
            LtraCase::Rlc => {
                let theta = 0.5 * (m.resist / (omega * m.induct)).atan();
                let root = (m.resist * m.resist + omega * omega * m.induct * m.induct).sqrt();
                let mut mag = (omega * m.capac / root).sqrt();
                let y0 = (mag * theta.cos(), mag * theta.sin());
                let theta = std::f64::consts::PI / 2. - theta;
                mag *= root;
                (y0.0, y0.1, mag * theta.cos(), mag * theta.sin())
            }
            LtraCase::Rc => {
                let y = (0.5 * omega * m.cbyr).sqrt();
                let l = (0.5 * omega * m.resist * m.capac).sqrt();
                (y, y, l, l)
            }
            // RG lines stamp their DC load instead (stamp_small_signal).
            LtraCase::Rg => return Vec::new(),
        };
        let exparg_r = -lambda_r * m.length;
        let exparg_i = -lambda_i * m.length;
        let explambda_r = exparg_r.exp() * exparg_i.cos();
        let explambda_i = exparg_r.exp() * exparg_i.sin();
        let y0exp_r = y0_r * explambda_r - y0_i * explambda_i;
        let y0exp_i = y0_r * explambda_i + y0_i * explambda_r;
        let (p1, n1, p2, n2, b1, b2) = (
            self.pos1(),
            self.neg1(),
            self.pos2(),
            self.neg2(),
            self.ibr1(),
            self.ibr2(),
        );
        vec![
            (b1, p1, y0_r, y0_i),
            (b1, n1, -y0_r, -y0_i),
            (b1, b1, -1.0, 0.0),
            (b1, p2, -y0exp_r, -y0exp_i),
            (b1, n2, y0exp_r, y0exp_i),
            (b1, b2, -explambda_r, -explambda_i),
            (b2, p2, y0_r, y0_i),
            (b2, n2, -y0_r, -y0_i),
            (b2, b2, -1.0, 0.0),
            (b2, p1, -y0exp_r, -y0exp_i),
            (b2, n1, y0exp_r, y0exp_i),
            (b2, b1, -explambda_r, -explambda_i),
            (p1, b1, 1.0, 0.0),
            (n1, b1, -1.0, 0.0),
            (p2, b2, 1.0, 0.0),
            (n2, b2, -1.0, 0.0),
        ]
    }

    /// Small-signal assembly: the DC form at frequency zero (and always for
    /// RG, as `LTRAacLoad` calls the DC load), else `ltraacld.c` with `A +=
    /// Re`, `E += Im / omega`.
    fn stamp_small_signal(&self, context: &mut LinearContext<'_>, dc: bool) -> SpiceResult<()> {
        let frequency = context.model_context.frequency;
        let unknowns = context.unknowns;
        let row = |node: NodeId| unknowns.node_row(node);
        if !dc && frequency != 0. && self.model.case == LtraCase::Rg {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "AC analysis with RG lossy line {}: C's LTRAacLoad loads every LTRA model's \
                     DC stamp and returns early when it meets an RG model, which corrupts the \
                     AC load of any other lossy line in the circuit; the port cannot see the \
                     other lines from the device and refuses the RG line in AC",
                    self.name
                ),
                "src/spicelib/devices/ltra/ltraacld.c (LTRA_MOD_RG case)",
            ));
        }
        if dc || frequency == 0. || self.model.case == LtraCase::Rg {
            for (r, c, value) in self.dc_entries(context.model_context.gmin) {
                if let (Some(r), Some(c)) = (row(r), row(c)) {
                    context.system.a.add(r, c, value)?;
                }
            }
            return Ok(());
        }
        let omega = 2. * std::f64::consts::PI * frequency;
        for (r, c, re, im) in self.ac_entries(omega) {
            if !(re.is_finite() && im.is_finite()) {
                return Err(self.internal(format!(
                    "nonfinite AC coefficient at {frequency:e} Hz (ltraacld.c)"
                )));
            }
            if let (Some(r), Some(c)) = (row(r), row(c)) {
                context.system.a.add(r, c, re)?;
                if im != 0. {
                    context.system.e.add(r, c, im / omega)?;
                }
            }
        }
        Ok(())
    }

    fn unsupported(&self, analysis: &str) -> SpiceError {
        SpiceError::not_yet_ported(
            format!(
                "{analysis} with lossy transmission line {} (C has no LTRA routine for it)",
                self.name
            ),
            C_REFERENCE,
        )
    }

    /// `ltraacct.c`'s slope-change test on the attenuated waves of one port.
    fn slope_changed(&self, d1: Real, d2: Real) -> bool {
        (d1 - d2).abs() >= self.model.reltol * d1.abs().max(d2.abs()) + self.model.abstol
    }
}

impl Device for LossyLine {
    fn name(&self) -> &str {
        &self.name
    }

    fn designator(&self) -> char {
        'o'
    }

    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }

    fn small_signal_depends_on_frequency(&self) -> bool {
        // RG lines too, so that their AC refusal sees the frequency.
        true
    }

    fn delay_line(&self) -> Option<&dyn DelayLine> {
        Some(self)
    }

    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        match context.mode {
            AnalysisMode::OperatingPoint | AnalysisMode::DcSweep => {
                for (r, c, value) in self.dc_entries(context.gmin) {
                    context.stamp(r, c, value)?;
                }
            }
            AnalysisMode::Transient { time, .. } => {
                if self.model.case == LtraCase::Rg {
                    // ltraload.c loads RG lines with their DC two-port always.
                    for (r, c, value) in self.dc_entries(context.gmin) {
                        context.stamp(r, c, value)?;
                    }
                    return Ok(());
                }
                let history = context.states.delay_history().ok_or_else(|| {
                    SpiceError::circuit(format!(
                        "{}: transient load without a delay history (companion driver only)",
                        self.name
                    ))
                })?;
                let load = self.transient_load(history, time)?;
                for (r, c, value) in self.transient_entries(load.first) {
                    context.stamp(r, c, value)?;
                }
                context.stamp_rhs(self.ibr1(), load.inputs[0])?;
                context.stamp_rhs(self.ibr2(), load.inputs[1])?;
            }
            AnalysisMode::Ac { .. } => {
                return Err(SpiceError::circuit(format!(
                    "{}: the AC load is assembled by assemble_small_signal",
                    self.name
                )));
            }
        }
        Ok(())
    }

    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        self.stamp_small_signal(context, true)
    }

    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        _bias: &Vector,
    ) -> SpiceResult<()> {
        self.stamp_small_signal(context, false)
    }

    fn assemble_pole_zero(
        &self,
        _context: &mut LinearContext<'_>,
        _bias: &Vector,
    ) -> SpiceResult<()> {
        Err(SpiceError::Unsupported {
            feature: format!(
                "pole-zero analysis of lossy transmission line {}: exp(-lambda(s) len) is not a \
                 polynomial pencil, and C has no LTRA pole-zero load (DEVpzLoad is NULL in \
                 ltrainit.c; PZinit's transmission-line check finds only TRA lines, so ngspice \
                 silently leaves the lossy line out of the matrix)",
                self.name
            ),
            location: Some(self.location.clone()),
        })
    }

    fn noise(
        &self,
        _context: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        Err(self.unsupported(".noise"))
    }

    fn distortion(
        &self,
        _context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        Err(self.unsupported(".disto"))
    }
}

impl DelayLine for LossyLine {
    fn width(&self) -> usize {
        4
    }

    /// The `t = 0` sample (C stores the operating point at `CKTtimeIndex`
    /// 0 in `LTRAaccept` and copies it into `LTRAinitVolt/Cur` at the first
    /// `MODEINITTRAN` load).
    fn start(&self, context: &DelayContext<'_>) -> SpiceResult<DelayUpdate> {
        if context.initial_conditions {
            return Err(SpiceError::not_yet_ported(
                format!(".tran uic with lossy transmission line {}", self.name),
                "src/spicelib/devices/ltra/ltraload.c (MODEINITTRAN | MODEUIC)",
            ));
        }
        Ok(DelayUpdate {
            reset: Some(vec![(context.time, self.port_values(context).to_vec())]),
            ..DelayUpdate::default()
        })
    }

    /// `ltraacct.c` (without `trytocompact`): record the point, and request a
    /// breakpoint at the previous time plus `TD` where an attenuated wave
    /// changes slope.
    fn accept(
        &self,
        history: &DelayHistory,
        context: &DelayContext<'_>,
    ) -> SpiceResult<DelayUpdate> {
        let m = &self.model;
        let values = self.port_values(context);
        let count = history.len();
        if count == 0 {
            return Err(SpiceError::circuit(format!(
                "{}: delay history too short",
                self.name
            )));
        }
        let time = context.time;
        let at = |index: usize| history.time(index).unwrap_or(Real::NAN);
        let value = |index: usize, k: usize| history.value(index, k).unwrap_or(Real::NAN);
        let wave = |v: Real, i: Real| (v + i * m.imped) * m.attenuation;
        // The new sample is index `count`; C's CKTtimeIndex.
        let previous = count - 1;
        let v1 = wave(values[0], values[1]);
        let v2 = wave(value(previous, 0), value(previous, 1));
        let v3 = if count < 2 {
            v2
        } else {
            wave(value(previous - 1, 0), value(previous - 1, 1))
        };
        let v4 = wave(values[2], values[3]);
        let v5 = wave(value(previous, 2), value(previous, 3));
        let v6 = if count < 2 {
            v5
        } else {
            wave(value(previous - 1, 2), value(previous - 1, 3))
        };
        let h = time - at(previous);
        let d1 = (v1 - v2) / h;
        let d2 = if count < 2 {
            0.
        } else {
            (v2 - v3) / (at(previous) - at(previous - 1))
        };
        let d3 = (v4 - v5) / h;
        let d4 = if count < 2 {
            0.
        } else {
            (v5 - v6) / (at(previous) - at(previous - 1))
        };
        let check = |a: Real, b: Real, c: Real| {
            a.max(b).max(c) - a.min(b).min(c)
                >= (50.0 * (context.reltol / 3.0 * (a + b + c) + context.abstol)).abs()
        };
        let mut update = DelayUpdate {
            append: Some((time, values.to_vec())),
            ..DelayUpdate::default()
        };
        let fire = (self.slope_changed(d1, d2) && check(v1, v2, v3))
            || (self.slope_changed(d3, d4) && check(v4, v5, v6));
        if fire {
            let when = at(previous) + m.td;
            // CKTsetBreak: a request equal to now (3 ULPs) is ignored, one
            // before now is C's "breakpoint in the past" error.
            if !almost_equal_ulps(when, time, 3) {
                if when < time {
                    return Err(self.internal(format!(
                        "breakpoint in the past at {when:e} (now {time:e}; ltraacct.c \
                         CKTsetBreak fails the same way)"
                    )));
                }
                update.breakpoints.push(when);
            }
        }
        Ok(update)
    }

    /// `ltratrun.c` (the reachable part: `LTRAlteConType` is always
    /// `nocontrol` from a deck).
    fn timestep_limit(
        &self,
        history: &DelayHistory,
        context: &DelayContext<'_>,
    ) -> SpiceResult<Option<Real>> {
        let m = &self.model;
        if !matches!(m.case, LtraCase::Lc | LtraCase::Rlc) {
            return Ok(None);
        }
        let mut bound = None;
        if m.step_limit {
            bound = Some(m.td);
        } else {
            let count = history.len();
            if count < 2 {
                return Err(SpiceError::circuit(format!(
                    "{}: delay history too short",
                    self.name
                )));
            }
            let index = count - 1;
            let at = |i: usize| history.time(i).unwrap_or(Real::NAN);
            let value = |i: usize, k: usize| history.value(i, k).unwrap_or(Real::NAN);
            let values = self.port_values(context);
            let wave = |v: Real, i: Real| (v * m.admit + i) * m.attenuation;
            let i1 = wave(values[2], values[3]);
            let i2 = wave(value(index, 2), value(index, 3));
            let i3 = wave(value(index - 1, 2), value(index - 1, 3));
            let i4 = wave(values[0], values[1]);
            let i5 = wave(value(index, 0), value(index, 1));
            let i6 = wave(value(index - 1, 0), value(index - 1, 1));
            let h = context.time - at(index);
            let h_old = at(index) - at(index - 1);
            let (d1, d2) = ((i1 - i2) / h, (i2 - i3) / h_old);
            let (d3, d4) = ((i4 - i5) / h, (i5 - i6) / h_old);
            if self.slope_changed(d1, d2) || self.slope_changed(d3, d4) {
                bound = Some(m.td);
            }
        }
        if m.case == LtraCase::Rlc && !m.trunc_dont_cut {
            // ltraset.c: compactrel/compactabs default to reltol/abstol.
            let reltol = if m.st_line_reltol == 0. {
                context.reltol
            } else {
                m.st_line_reltol
            };
            let abstol = if m.st_line_abstol == 0. {
                context.abstol
            } else {
                m.st_line_abstol
            };
            let safe = convolution::rlc_max_safe_step(m.td, m.alpha, m.beta, reltol, abstol);
            bound = Some(bound.map_or(safe, |b: Real| b.min(safe)));
        }
        Ok(bound)
    }
}

/// `AlmostEqualUlps(a, b, max_ulps)` (`src/misc/equality.c`).
fn almost_equal_ulps(a: Real, b: Real, max_ulps: i64) -> bool {
    if a == b {
        return true;
    }
    let ordered = |x: Real| {
        let bits = x.to_bits() as i64;
        if bits < 0 { i64::MIN - bits } else { bits }
    };
    (ordered(a) - ordered(b)).unsigned_abs() <= max_ulps.unsigned_abs()
}
