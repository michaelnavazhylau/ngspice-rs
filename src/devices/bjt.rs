//! Gummel-Poon level-1 bipolar junction transistor.
//!
//! C references (behaviour only, reimplemented):
//! - `bjt/bjt.c` / `bjtmpar.c`: model and instance setters, including aliases.
//! - `bjt/bjtsetup.c`: defaults and the internal collector/base/emitter nodes
//!   created for nonzero `RC`/`RB`/`RE`.
//! - `bjt/bjttemp.c`: temperature, area and `TLEV`/`TLEVC` scaling.
//! - `bjt/bjtload.c`: base charge `qb` (Early `VAF`/`VAR`, high injection
//!   `IKF`/`IKR`/`NKF`), `ISE`/`ISC` leakage, bias-dependent base resistance
//!   (`RB`/`RBM`/`IRB`), bias-dependent transit time (`XTF`/`VTF`/`ITF`),
//!   the split base-collector capacitance (`XCJC`) and the substrate junction
//!   (`ISS`/`NS`, `CJS`/`VJS`/`MJS`, `SUBS`).
//! - `bjt/bjtacld.c`: the small-signal stamp.
//! - `bjt/bjttrunc.c`: charges taking part in truncation-error control.
//!
//! The Newton load stamps the *exact* Jacobian of the C equations, including
//! the bias dependence of the base resistance. C's `bjtload.c` stamps only the
//! conductance `gx` there; both iterations share the same fixed point.
//! [`Device::assemble_small_signal`] deliberately reproduces C's AC stamp,
//! which omits the `d(gx)/dV` terms, because that is what `.ac` computes; a
//! load whose trial asks for C's matrix
//! ([`crate::devices::DeviceState::c_jacobian`], used by `.tf`) omits them too.
//!
//! Not ported, and rejected with [`SpiceError::NotYetPorted`]: excess phase
//! (`PTF` with `TF != 0`), Kull's quasi-saturation model (`RCO`, `VO`, `GAMMA`,
//! `QCO`, ...), safe-operating-area limits (`*_MAX`, `RTH0`). The `.noise`
//! generators of `bjtnoise.c` (resistor thermal, collector/base shot and
//! `KF`/`AF` flicker noise) are ported through [`Device::noise`].
//!
//! Newton limiting follows `bjtload.c` through [`crate::devices::limiting`]:
//! `MODEINITJCT` starts at `vbe = tVcrit` (all zero for an `OFF` instance,
//! which is also held at zero through `MODEINITFIX`); the `uic` initial load
//! starts at `vbe = ICVBE`, `vbc = vbx = ICVBE - ICVCE`, `vsub = 0`, unset
//! components defaulted from the external terminals as `bjtgetic.c` does;
//! later loads apply `DEVpnjlim`
//! to `vbe`, `vbc` and `vsub`. C's `MODEINITPRED` extrapolation, bypass and
//! the quasi-saturation `vbcx`/`vrci` limits (a rejected model) are not ported.
use crate::devices::limiting::{self, Limiter, Linearization};
use crate::devices::noise::{DeviceNoise, NoiseContext, NoiseFamily, NoiseKind, NoiseSource};
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U, ScalarValues,
};
use crate::devices::{
    Device, LinearContext, ModelContext, ModelFamily, ResolvedModel, StampContext,
};
use crate::maths::Vector;
use crate::netlist::ast::{DeviceInstance, ParameterAssignment};
use crate::primitives::{NodeId, NodeKind, NodeTable, Real, SpiceError, SpiceResult};

/// ngspice `CONSTboltz`.
const BOLTZMANN: Real = 1.38064852e-23;
/// ngspice `CHARGE`.
const CHARGE: Real = 1.6021766208e-19;
/// ngspice `CONSTKoverQ`.
const K_OVER_Q: Real = BOLTZMANN / CHARGE;
/// ngspice `REFTEMP` (27 C in kelvin).
const REFTEMP: Real = 27. + CELSIUS_TO_KELVIN;
/// ngspice `CONSTCtoK`.
const CELSIUS_TO_KELVIN: Real = 273.15;
/// ngspice `MAX_EXP_ARG`, the substrate exponential clamp of `bjtload.c`.
const MAX_EXP_ARG: Real = 709.;

const fn p(name: &'static str, unit: U, domain: D, default: Option<Real>) -> P {
    P {
        name,
        unit,
        domain,
        default,
    }
}

/// The supported model setters. Defaults follow `bjtsetup.c`/`bjttemp.c`;
/// values without a default are "not given" (C `*Given` flags).
const MODEL: &[P] = &[
    p("is", U::Ampere, D::Positive, Some(1e-16)),
    p("ibe", U::Ampere, D::NonNegative, None),
    p("ibc", U::Ampere, D::NonNegative, None),
    p("bf", U::Dimensionless, D::Positive, Some(100.)),
    p("nf", U::Dimensionless, D::Positive, Some(1.)),
    p("vaf", U::Volt, D::NonNegative, Some(0.)),
    p("ikf", U::Ampere, D::NonNegative, Some(0.)),
    p("nkf", U::Dimensionless, D::Positive, None),
    p("ise", U::Ampere, D::NonNegative, Some(0.)),
    p("ne", U::Dimensionless, D::Positive, Some(1.5)),
    p("br", U::Dimensionless, D::Positive, Some(1.)),
    p("nr", U::Dimensionless, D::Positive, Some(1.)),
    p("var", U::Volt, D::NonNegative, Some(0.)),
    p("ikr", U::Ampere, D::NonNegative, Some(0.)),
    p("isc", U::Ampere, D::NonNegative, Some(0.)),
    p("nc", U::Dimensionless, D::Positive, Some(2.)),
    p("rb", U::Ohm, D::NonNegative, Some(0.)),
    p("irb", U::Ampere, D::NonNegative, Some(0.)),
    p("rbm", U::Ohm, D::NonNegative, None),
    p("re", U::Ohm, D::NonNegative, Some(0.)),
    p("rc", U::Ohm, D::NonNegative, Some(0.)),
    p("cje", U::Farad, D::NonNegative, Some(0.)),
    p("vje", U::Volt, D::Positive, Some(0.75)),
    p("mje", U::Dimensionless, D::NonNegative, Some(0.33)),
    p("tf", U::Second, D::NonNegative, Some(0.)),
    p("xtf", U::Dimensionless, D::NonNegative, Some(0.)),
    p("vtf", U::Volt, D::NonNegative, Some(0.)),
    p("itf", U::Ampere, D::NonNegative, Some(0.)),
    p("ptf", U::Dimensionless, D::Finite, Some(0.)),
    p("cjc", U::Farad, D::NonNegative, Some(0.)),
    p("vjc", U::Volt, D::Positive, Some(0.75)),
    p("mjc", U::Dimensionless, D::NonNegative, Some(0.33)),
    p("xcjc", U::Dimensionless, D::Finite, Some(1.)),
    p("tr", U::Second, D::NonNegative, Some(0.)),
    p("cjs", U::Farad, D::NonNegative, Some(0.)),
    p("vjs", U::Volt, D::Positive, Some(0.75)),
    p("mjs", U::Dimensionless, D::NonNegative, Some(0.)),
    p("xtb", U::Dimensionless, D::Finite, Some(0.)),
    p("eg", U::Volt, D::Positive, Some(1.11)),
    p("xti", U::Dimensionless, D::Finite, Some(3.)),
    p("fc", U::Dimensionless, D::NonNegative, Some(0.5)),
    p("iss", U::Ampere, D::NonNegative, None),
    p("ns", U::Dimensionless, D::Positive, Some(1.)),
    p("tnom", U::Celsius, D::Temperature, None),
    p("subs", U::Dimensionless, D::Finite, None),
    p("tlev", U::Dimensionless, D::Finite, Some(0.)),
    p("tlevc", U::Dimensionless, D::Finite, Some(0.)),
    p("tbf1", U::InverseKelvin, D::Finite, None),
    p("tbf2", U::InverseKelvinSquared, D::Finite, None),
    p("tbr1", U::InverseKelvin, D::Finite, None),
    p("tbr2", U::InverseKelvinSquared, D::Finite, None),
    p("tikf1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tikf2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tikr1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tikr2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tirb1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tirb2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tnc1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tnc2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tne1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tne2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tnf1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tnf2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tnr1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tnr2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("trb1", U::InverseKelvin, D::Finite, Some(0.)),
    p("trb2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("trc1", U::InverseKelvin, D::Finite, Some(0.)),
    p("trc2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tre1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tre2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("trm1", U::InverseKelvin, D::Finite, Some(0.)),
    p("trm2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tvaf1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tvaf2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tvar1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tvar2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("ctc", U::InverseKelvin, D::Finite, Some(0.)),
    p("cte", U::InverseKelvin, D::Finite, Some(0.)),
    p("cts", U::InverseKelvin, D::Finite, Some(0.)),
    p("tvjc", U::Volt, D::Finite, Some(0.)),
    p("tvje", U::Volt, D::Finite, Some(0.)),
    p("tvjs", U::Volt, D::Finite, Some(0.)),
    p("titf1", U::InverseKelvin, D::Finite, Some(0.)),
    p("titf2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("ttf1", U::InverseKelvin, D::Finite, Some(0.)),
    p("ttf2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("ttr1", U::InverseKelvin, D::Finite, Some(0.)),
    p("ttr2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tmje1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tmje2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tmjc1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tmjc2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tmjs1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tmjs2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tns1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tns2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tis1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tis2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tise1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tise2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tisc1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tisc2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("tiss1", U::InverseKelvin, D::Finite, Some(0.)),
    p("tiss2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    // bjtnoise.c flicker law; bjtsetup.c defaults KF = 0, AF = 1.
    p("kf", U::Dimensionless, D::Finite, Some(0.)),
    p("af", U::Dimensionless, D::Finite, Some(1.)),
];

/// `bjt.c` alias setters (`IOPR`/`IOPAR`) and their canonical parameter.
const ALIASES: &[(&str, &str)] = &[
    ("tref", "tnom"),
    ("va", "vaf"),
    ("ik", "ikf"),
    ("nk", "nkf"),
    ("c2", "ise"),
    ("vb", "var"),
    ("c4", "isc"),
    ("pe", "vje"),
    ("me", "mje"),
    ("pc", "vjc"),
    ("mc", "mjc"),
    ("csub", "cjs"),
    ("ccs", "cjs"),
    ("ps", "vjs"),
    ("ms", "mjs"),
    ("trb", "trb1"),
    ("trc", "trc1"),
    ("tre", "tre1"),
];

/// Model setters C accepts whose physics is not ported, with the C file
/// that defines it.
const UNPORTED_MODEL: &[(&str, &str)] = &[
    ("rco", "Kull quasi-saturation, bjt/bjtload.c and bjttemp.c"),
    ("vo", "Kull quasi-saturation, bjt/bjtload.c and bjttemp.c"),
    (
        "gamma",
        "Kull quasi-saturation, bjt/bjtload.c and bjttemp.c",
    ),
    ("qco", "Kull quasi-saturation, bjt/bjtload.c and bjttemp.c"),
    ("quasimod", "Kull quasi-saturation, bjt/bjttemp.c"),
    ("vg", "Kull quasi-saturation, bjt/bjttemp.c"),
    ("cn", "Kull quasi-saturation, bjt/bjttemp.c"),
    ("d", "Kull quasi-saturation, bjt/bjttemp.c"),
    ("vbe_max", "BJT safe-operating-area check, bjt/bjtsoachk.c"),
    ("vbc_max", "BJT safe-operating-area check, bjt/bjtsoachk.c"),
    ("vce_max", "BJT safe-operating-area check, bjt/bjtsoachk.c"),
    ("pd_max", "BJT safe-operating-area check, bjt/bjtsoachk.c"),
    ("ic_max", "BJT safe-operating-area check, bjt/bjtsoachk.c"),
    ("ib_max", "BJT safe-operating-area check, bjt/bjtsoachk.c"),
    ("te_max", "BJT safe-operating-area check, bjt/bjtsoachk.c"),
    ("rth0", "BJT safe-operating-area check, bjt/bjtsoachk.c"),
];

const INSTANCE: &[P] = &[
    p("area", U::Dimensionless, D::Positive, Some(1.)),
    p("areab", U::Dimensionless, D::Positive, None),
    p("areac", U::Dimensionless, D::Positive, None),
    p("m", U::Dimensionless, D::Positive, Some(1.)),
    p("temp", U::Celsius, D::Temperature, None),
    p("dtemp", U::Celsius, D::Finite, Some(0.)),
];

/// The `IC` vector components (`bjtpar.c`): `ICVBE`, `ICVCE`.
const IC_COMPONENTS: [&str; 2] = ["icvbe", "icvce"];

fn canonical(parameter: &ParameterAssignment) -> ParameterAssignment {
    let mut parameter = parameter.clone();
    if let Some((_, name)) = ALIASES
        .iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(&parameter.name))
    {
        (*name).clone_into(&mut parameter.name);
    }
    parameter
}

fn get(values: &ScalarValues, name: &str) -> SpiceResult<Real> {
    values
        .get(name)
        .map(|v| v.value)
        .ok_or_else(|| SpiceError::circuit(format!("missing BJT schema default {name}")))
}

/// An explicitly given value (C `*Given`), not a schema default.
fn given(values: &ScalarValues, name: &str) -> Option<Real> {
    values
        .get(name)
        .filter(|v| v.location.is_some())
        .map(|v| v.value)
}

/// A quadratic temperature coefficient pair `(c1, c2)` (`1 + c1 dt + c2 dt^2`).
type TempCo = [Real; 2];

fn poly(c: TempCo, dt: Real) -> Real {
    1. + c[0] * dt + c[1] * dt * dt
}

/// Validated model data at the parameter-measurement temperature.
#[derive(Debug, Clone, Copy)]
struct Model {
    is: Real,
    /// `IBE`/`IBC`, used only when both are given (`bjttemp.c`).
    ibe_ibc: Option<(Real, Real)>,
    bf: Real,
    nf: Real,
    vaf: Real,
    ikf: Real,
    nkf: Option<Real>,
    ise: Real,
    ne: Real,
    br: Real,
    nr: Real,
    var: Real,
    ikr: Real,
    isc: Real,
    nc: Real,
    rb: Real,
    irb: Real,
    rbm: Real,
    re: Real,
    rc: Real,
    cje: Real,
    vje: Real,
    mje: Real,
    tf: Real,
    xtf: Real,
    vtf: Real,
    itf: Real,
    cjc: Real,
    vjc: Real,
    mjc: Real,
    xcjc: Real,
    tr: Real,
    cjs: Real,
    vjs: Real,
    mjs: Real,
    xtb: Real,
    eg: Real,
    xti: Real,
    fc: Real,
    iss: Option<Real>,
    ns: Real,
    /// Kelvin, `None` for the circuit nominal temperature.
    tnom: Option<Real>,
    tlev: u8,
    tlevc: u8,
    tbf: Option<TempCo>,
    tbr: Option<TempCo>,
    tikf: TempCo,
    tikr: TempCo,
    tirb: TempCo,
    tnc: TempCo,
    tne: TempCo,
    tnf: TempCo,
    tnr: TempCo,
    trb: TempCo,
    trc: TempCo,
    tre: TempCo,
    trm: TempCo,
    tvaf: TempCo,
    tvar: TempCo,
    titf: TempCo,
    ttf: TempCo,
    ttr: TempCo,
    tmje: TempCo,
    tmjc: TempCo,
    tmjs: TempCo,
    tns: TempCo,
    tis: TempCo,
    tise: TempCo,
    tisc: TempCo,
    tiss: TempCo,
    ctc: Real,
    cte: Real,
    cts: Real,
    tvjc: Real,
    tvje: Real,
    tvjs: Real,
    /// Flicker-noise coefficient and exponent (`bjtnoise.c`).
    kf: Real,
    af: Real,
}

fn integer_selector(values: &ScalarValues, name: &str, allowed: &[u8]) -> SpiceResult<u8> {
    let raw = get(values, name)?;
    allowed
        .iter()
        .copied()
        .find(|v| Real::from(*v) == raw)
        .ok_or_else(|| {
            SpiceError::circuit(format!(
                "BJT {name}={raw} is not one of the ported selectors {allowed:?}"
            ))
        })
}

impl Model {
    fn from_values(m: &ScalarValues) -> SpiceResult<Self> {
        let pair = |a: &str, b: &str| -> SpiceResult<TempCo> { Ok([get(m, a)?, get(m, b)?]) };
        // bjttemp.c: BF/BR use the TBF/TBR polynomial when either is given.
        let optional_pair = |a: &str, b: &str| -> Option<TempCo> {
            match (given(m, a), given(m, b)) {
                (None, None) => None,
                (x, y) => Some([x.unwrap_or(0.), y.unwrap_or(0.)]),
            }
        };
        let is = get(m, "is")?;
        // bjtsetup.c: a leakage current above 1e-4 is a multiple of IS.
        let leak = |name: &str| -> SpiceResult<Real> {
            let value = get(m, name)?;
            Ok(if value > 1e-4 { is * value } else { value })
        };
        let rb = get(m, "rb")?;
        let fc = get(m, "fc")?;
        let xcjc = get(m, "xcjc")?;
        let model = Self {
            is,
            ibe_ibc: given(m, "ibe").zip(given(m, "ibc")),
            bf: get(m, "bf")?,
            nf: get(m, "nf")?,
            vaf: get(m, "vaf")?,
            ikf: get(m, "ikf")?,
            // bjtsetup.c clamps NKF to at most 1.
            nkf: given(m, "nkf").map(|v| v.min(1.)),
            ise: leak("ise")?,
            ne: get(m, "ne")?,
            br: get(m, "br")?,
            nr: get(m, "nr")?,
            var: get(m, "var")?,
            ikr: get(m, "ikr")?,
            isc: leak("isc")?,
            nc: get(m, "nc")?,
            rb,
            irb: get(m, "irb")?,
            rbm: given(m, "rbm").unwrap_or(rb),
            re: get(m, "re")?,
            rc: get(m, "rc")?,
            cje: get(m, "cje")?,
            vje: get(m, "vje")?,
            mje: get(m, "mje")?,
            tf: get(m, "tf")?,
            xtf: get(m, "xtf")?,
            vtf: get(m, "vtf")?,
            itf: get(m, "itf")?,
            cjc: get(m, "cjc")?,
            vjc: get(m, "vjc")?,
            mjc: get(m, "mjc")?,
            // bjtsetup.c clamps XCJC into [0, 1].
            xcjc: xcjc.clamp(0., 1.),
            tr: get(m, "tr")?,
            cjs: get(m, "cjs")?,
            vjs: get(m, "vjs")?,
            mjs: get(m, "mjs")?,
            xtb: get(m, "xtb")?,
            eg: get(m, "eg")?,
            xti: get(m, "xti")?,
            // bjttemp.c limits FC to 0.9999.
            fc: fc.min(0.9999),
            iss: given(m, "iss"),
            ns: get(m, "ns")?,
            tnom: given(m, "tnom").map(|t| t + CELSIUS_TO_KELVIN),
            tlev: integer_selector(m, "tlev", &[0, 1, 3])?,
            tlevc: integer_selector(m, "tlevc", &[0, 1])?,
            tbf: optional_pair("tbf1", "tbf2"),
            tbr: optional_pair("tbr1", "tbr2"),
            tikf: pair("tikf1", "tikf2")?,
            tikr: pair("tikr1", "tikr2")?,
            tirb: pair("tirb1", "tirb2")?,
            tnc: pair("tnc1", "tnc2")?,
            tne: pair("tne1", "tne2")?,
            tnf: pair("tnf1", "tnf2")?,
            tnr: pair("tnr1", "tnr2")?,
            trb: pair("trb1", "trb2")?,
            trc: pair("trc1", "trc2")?,
            tre: pair("tre1", "tre2")?,
            trm: pair("trm1", "trm2")?,
            tvaf: pair("tvaf1", "tvaf2")?,
            tvar: pair("tvar1", "tvar2")?,
            titf: pair("titf1", "titf2")?,
            ttf: pair("ttf1", "ttf2")?,
            ttr: pair("ttr1", "ttr2")?,
            tmje: pair("tmje1", "tmje2")?,
            tmjc: pair("tmjc1", "tmjc2")?,
            tmjs: pair("tmjs1", "tmjs2")?,
            tns: pair("tns1", "tns2")?,
            tis: pair("tis1", "tis2")?,
            tise: pair("tise1", "tise2")?,
            tisc: pair("tisc1", "tisc2")?,
            tiss: pair("tiss1", "tiss2")?,
            ctc: get(m, "ctc")?,
            cte: get(m, "cte")?,
            cts: get(m, "cts")?,
            tvjc: get(m, "tvjc")?,
            tvje: get(m, "tvje")?,
            tvjs: get(m, "tvjs")?,
            kf: get(m, "kf")?,
            af: get(m, "af")?,
        };
        // Grading coefficients of 1 or more divide by zero in the depletion
        // charge; C clamps them (with tempco) to 0.999, the port additionally
        // rejects the raw value as malformed. FC >= 1 is rejected likewise.
        if model.mje >= 1. || model.mjc >= 1. || model.mjs >= 1. || fc >= 1. {
            return Err(SpiceError::circuit("BJT MJE/MJC/MJS/FC must be below 1"));
        }
        Ok(model)
    }
}

/// Instance geometry and temperature.
#[derive(Debug, Clone, Copy)]
struct Instance {
    area: Real,
    areab: Real,
    areac: Real,
    multiplier: Real,
    /// Kelvin, `None` for the circuit temperature plus `dtemp`.
    temp: Option<Real>,
    dtemp: Real,
}

/// Quantities at the instance temperature, scaled by area (`bjttemp.c`).
#[derive(Debug, Clone, Copy)]
struct Thermal {
    vt: Real,
    /// `BJTtSatCur` (`area * IS` at temperature), which sets `BJTtVcrit`.
    is: Real,
    is_be: Real,
    is_bc: Real,
    ise: Real,
    isc: Real,
    iss: Option<Real>,
    nf: Real,
    nr: Real,
    ne: Real,
    nc: Real,
    ns: Real,
    nkf: Option<Real>,
    bf: Real,
    br: Real,
    inv_vaf: Real,
    inv_var: Real,
    inv_ikf: Real,
    inv_ikr: Real,
    rb: Real,
    rbm: Real,
    irb: Real,
    gc: Real,
    ge: Real,
    tf: Real,
    tr: Real,
    xtf: Real,
    vtf_factor: Real,
    itf: Real,
    cje: Real,
    vje: Real,
    mje: Real,
    cjc: Real,
    vjc: Real,
    mjc: Real,
    xcjc: Real,
    cjs: Real,
    vjs: Real,
    mjs: Real,
    fc_vje: Real,
    fc_vjc: Real,
    f1: Real,
    f2: Real,
    f3: Real,
    f5: Real,
    f6: Real,
    f7: Real,
}

/// `bjttemp.c`'s silicon band-gap built-in-potential shift `pbfact` at `t`.
fn pbfact(t: Real) -> Real {
    let vt = K_OVER_Q * t;
    let egfet = 1.16 - (7.02e-4 * t * t) / (t + 1108.);
    let arg = -egfet / (2. * BOLTZMANN * t) + 1.1150877 / (BOLTZMANN * (REFTEMP + REFTEMP));
    -2. * vt * (1.5 * (t / REFTEMP).ln() + CHARGE * arg)
}

impl Model {
    #[allow(clippy::too_many_lines)]
    fn thermal(
        &self,
        instance: &Instance,
        vertical: bool,
        context: &ModelContext,
    ) -> SpiceResult<Thermal> {
        let tnom = self
            .tnom
            .unwrap_or(context.nominal_temperature + CELSIUS_TO_KELVIN);
        let t = instance
            .temp
            .unwrap_or(context.temperature + CELSIUS_TO_KELVIN + instance.dtemp);
        if !(t.is_finite() && t > 0. && tnom.is_finite() && tnom > 0.) {
            return Err(SpiceError::circuit("invalid BJT temperature"));
        }
        let Instance {
            area, areab, areac, ..
        } = *instance;
        // Base/collector area of the base-collector and substrate junctions:
        // AREAB for a vertical device, AREAC for a lateral one (bjttemp.c).
        let (bc_area, sub_area) = if vertical {
            (areab, areac)
        } else {
            (areac, areab)
        };
        let dt = t - tnom;
        let vt = t * K_OVER_Q;
        let fact1 = tnom / REFTEMP;
        let fact2 = t / REFTEMP;
        let pbfact1 = pbfact(tnom);
        let pbfact = pbfact(t);
        let ratlog = (t / tnom).ln();
        let ratio1 = t / tnom - 1.;
        let factlog = ratio1 * self.eg / vt + self.xti * ratlog;
        let (is, is_be, mut is_bc, mut iss) = if self.tlev == 3 {
            let exponent = poly(self.tis, dt);
            let is = area * self.is.powf(exponent);
            let (be, bc) = match self.ibe_ibc {
                Some((ibe, ibc)) => (area * ibe.powf(exponent), ibc.powf(exponent)),
                None => (is, is / area),
            };
            let iss = self.iss.map(|v| v.powf(poly(self.tiss, dt)));
            (is, be, bc, iss)
        } else {
            let is = area * self.is * factlog.exp();
            let (be, bc) = match self.ibe_ibc {
                Some((ibe, ibc)) => (
                    area * ibe * (factlog / self.nf).exp(),
                    ibc * (factlog / self.nr).exp(),
                ),
                None => (is, is / area),
            };
            let iss = self.iss.map(|v| v * (factlog / self.ns).exp());
            (is, be, bc, iss)
        };
        is_bc *= bc_area;
        if let Some(value) = iss.as_mut() {
            *value *= if self.ibe_ibc.is_some() {
                sub_area
            } else {
                area
            };
        }
        let bfactor = match self.tlev {
            0 => (ratlog * self.xtb).exp(),
            1 => 1. + self.xtb * dt,
            _ => 1.,
        };
        let bf = self
            .tbf
            .map_or(self.bf * bfactor, |c| self.bf * poly(c, dt));
        let br = self
            .tbr
            .map_or(self.br * bfactor, |c| self.br * poly(c, dt));
        let (ise, isc) = if self.tlev == 3 {
            (
                area * self.ise.powf(poly(self.tise, dt)),
                self.isc.powf(poly(self.tisc, dt)),
            )
        } else {
            (
                area * self.ise * (factlog / self.ne).exp() / bfactor,
                self.isc * (factlog / self.nc).exp() / bfactor,
            )
        };
        let isc = isc * bc_area;
        let inverse = |value: Real, c: TempCo| {
            if value == 0. {
                0.
            } else {
                1. / (value * poly(c, dt))
            }
        };
        // bjttemp.c clamps the temperature-adjusted grading to 0.999.
        let grading = |value: Real, c: TempCo| (value * poly(c, dt)).min(0.999);
        let mje = grading(self.mje, self.tmje);
        let mjc = grading(self.mjc, self.tmjc);
        let mjs = grading(self.mjs, self.tmjs);
        let junction = |cap: Real, pot: Real, m: Real, ct: Real, tv: Real| {
            if self.tlevc == 1 {
                (cap * (1. + ct * dt), pot - tv * dt)
            } else {
                let pbo = (pot - pbfact1) / fact1;
                let gmaold = (pot - pbo) / pbo;
                let mut cap = cap / (1. + m * (4e-4 * (tnom - REFTEMP) - gmaold));
                let pot = fact2 * pbo + pbfact;
                let gmanew = (pot - pbo) / pbo;
                cap *= 1. + m * (4e-4 * (t - REFTEMP) - gmanew);
                (cap, pot)
            }
        };
        let (cje, vje) = junction(self.cje, self.vje, mje, self.cte, self.tvje);
        let (cjc, vjc) = junction(self.cjc, self.vjc, mjc, self.ctc, self.tvjc);
        let (cjs, vjs) = junction(self.cjs, self.vjs, mjs, self.cts, self.tvjs);
        let xfc = (1. - self.fc).ln();
        let thermal = Thermal {
            vt,
            is,
            is_be,
            is_bc,
            ise,
            isc,
            iss,
            nf: self.nf * poly(self.tnf, dt),
            nr: self.nr * poly(self.tnr, dt),
            ne: self.ne * poly(self.tne, dt),
            nc: self.nc * poly(self.tnc, dt),
            ns: self.ns * poly(self.tns, dt),
            nkf: self.nkf,
            bf,
            br,
            inv_vaf: inverse(self.vaf, self.tvaf),
            inv_var: inverse(self.var, self.tvar),
            inv_ikf: inverse(self.ikf, self.tikf) / area,
            inv_ikr: inverse(self.ikr, self.tikr) / area,
            rb: self.rb * poly(self.trb, dt) / area,
            rbm: self.rbm * poly(self.trm, dt) / area,
            irb: self.irb * poly(self.tirb, dt) * area,
            gc: inverse(self.rc, self.trc) * area,
            ge: inverse(self.re, self.tre) * area,
            tf: self.tf * poly(self.ttf, dt),
            tr: self.tr * poly(self.ttr, dt),
            xtf: self.xtf,
            vtf_factor: if self.vtf == 0. {
                0.
            } else {
                1. / (self.vtf * 1.44)
            },
            itf: self.itf * poly(self.titf, dt) * area,
            cje: cje * area,
            vje,
            mje,
            cjc: cjc * bc_area,
            vjc,
            mjc,
            xcjc: self.xcjc,
            cjs: cjs * sub_area,
            vjs,
            mjs,
            fc_vje: self.fc * vje,
            fc_vjc: self.fc * vjc,
            f1: vje * (1. - ((1. - mje) * xfc).exp()) / (1. - mje),
            f2: ((1. + mje) * xfc).exp(),
            f3: 1. - self.fc * (1. + mje),
            f5: vjc * (1. - ((1. - mjc) * xfc).exp()) / (1. - mjc),
            f6: ((1. + mjc) * xfc).exp(),
            f7: 1. - self.fc * (1. + mjc),
        };
        thermal.validate()?;
        Ok(thermal)
    }
}

/// First BJT state slot of the limited `vbe`, `vbc`, `vsub` (C `BJTvbe`,
/// `BJTvbc`, `BJTvsub`).
const BJT_LIMITED_SLOTS: usize = 8;
/// `bjtdefs.h` `VCRIT_DISABLED`: the substrate critical voltage without ISS.
const SUBSTRATE_VCRIT_DISABLED: Real = 50.;

impl Thermal {
    /// `bjtload.c`'s normalized junction voltages for this load:
    /// `MODEINITJCT` starts at `vbe = tVcrit`, `vbc = vsub = 0`; later loads
    /// apply `DEVpnjlim` (with `vt = kT/q`, no emission coefficient) to
    /// `vbe`, `vbc` (`tVcrit` from `area * IS`) and `vsub` (`tSubVcrit` from
    /// ISS, else `VCRIT_DISABLED`). `vbx` is never limited. `m` scales the
    /// stamps, not `tSatCur`. Quasi-saturation (`vbcx`, `vrci`) is not ported.
    ///
    /// `start` is the `uic` initial bias (from `ICVBE`/`ICVCE`) and `off` the
    /// instance flag, which holds every junction at zero in `MODEINITJCT`
    /// and `MODEINITFIX`.
    fn limit(
        &self,
        limiter: &mut Limiter,
        states: &crate::devices::DeviceState<'_>,
        raw: Bias,
        start: Bias,
        off: bool,
    ) -> Bias {
        let vcrit = limiting::critical_voltage(self.vt, self.is);
        if limiter.mode() == Linearization::InitialConditions {
            return start;
        }
        if limiter.holds_off(states, off) {
            return Bias {
                vbe: 0.,
                vbc: 0.,
                vbx: 0.,
                vsub: 0.,
            };
        }
        if limiter.mode() == Linearization::Initial {
            return Bias {
                vbe: vcrit,
                vbc: 0.,
                vbx: 0.,
                vsub: 0.,
            };
        }
        let sub_vcrit = self.iss.map_or(SUBSTRATE_VCRIT_DISABLED, |iss| {
            limiting::critical_voltage(self.vt, iss)
        });
        let mut limited = raw;
        for (index, (value, critical)) in [
            (&mut limited.vbe, vcrit),
            (&mut limited.vbc, vcrit),
            (&mut limited.vsub, sub_vcrit),
        ]
        .into_iter()
        .enumerate()
        {
            let previous = limiter.previous(states, BJT_LIMITED_SLOTS + index);
            *value = limiter.pn_junction(*value, previous, self.vt, critical);
        }
        limited
    }

    fn validate(&self) -> SpiceResult<()> {
        let values = [
            self.vt,
            self.is_be,
            self.is_bc,
            self.ise,
            self.isc,
            self.iss.unwrap_or(0.),
            self.nf,
            self.nr,
            self.ne,
            self.nc,
            self.ns,
            self.bf,
            self.br,
            self.inv_vaf,
            self.inv_var,
            self.inv_ikf,
            self.inv_ikr,
            self.rb,
            self.rbm,
            self.irb,
            self.gc,
            self.ge,
            self.tf,
            self.tr,
            self.itf,
            self.cje,
            self.vje,
            self.cjc,
            self.vjc,
            self.cjs,
            self.vjs,
            self.f1,
            self.f2,
            self.f3,
            self.f5,
            self.f6,
            self.f7,
        ];
        if values.iter().any(|v| !v.is_finite())
            || [
                self.nf, self.nr, self.ne, self.nc, self.ns, self.vje, self.vjc, self.vjs,
            ]
            .iter()
            .any(|v| *v <= 0.)
            || self.bf == 0.
            || self.br == 0.
        {
            return Err(SpiceError::Numerical {
                context: "BJT temperature model".into(),
                message: "nonfinite or out-of-domain temperature-adjusted parameter".into(),
            });
        }
        Ok(())
    }
}

/// A junction current and its derivative, with C's cubic reverse continuation
/// below `-3 vtn` (`bjtload.c`).
fn pn(v: Real, vtn: Real, is: Real) -> (Real, Real) {
    if v >= -3. * vtn {
        let e = (v / vtn).exp();
        (is * (e - 1.), is * e / vtn)
    } else {
        let arg = (3. * vtn / (v * std::f64::consts::E)).powi(3);
        (-is * (1. + arg), is * 3. * arg / v)
    }
}

/// Depletion charge and capacitance with the forward-bias linear-capacitance
/// continuation above `fcp` (`bjtload.c`'s `f1`/`f2`/`f3` form).
#[allow(clippy::too_many_arguments)]
fn depletion(
    v: Real,
    cz: Real,
    p: Real,
    m: Real,
    fcp: Real,
    f1: Real,
    f2: Real,
    f3: Real,
) -> (Real, Real) {
    if v < fcp {
        let arg = 1. - v / p;
        let sarg = (-m * arg.ln()).exp();
        (p * cz * (1. - arg * sarg) / (1. - m), cz * sarg)
    } else {
        let czf2 = cz / f2;
        (
            cz * f1 + czf2 * (f3 * (v - fcp) + (m / (p + p)) * (v * v - fcp * fcp)),
            czf2 * (f3 + m * v / p),
        )
    }
}

/// Normalized branch voltages: `vbe`, `vbc` across the intrinsic junctions,
/// `vbx` from the external base to the internal collector and `vsub` across
/// the substrate junction, each multiplied by the device polarity (and the
/// substrate geometry sign for `vsub`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Bias {
    vbe: Real,
    vbc: Real,
    vbx: Real,
    vsub: Real,
}

/// A value with its derivatives with respect to `vbe` and `vbc`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Dual {
    value: Real,
    dvbe: Real,
    dvbc: Real,
}

/// One evaluation of the normalized equations at a [`Bias`], per unit `m`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Evaluation {
    /// Transport current from the internal collector to the internal emitter.
    transport: Dual,
    /// Base-emitter current `cbe/BF + cben` (including junction gmin).
    base_emitter: Dual,
    /// Base-collector current `cbc/BR + cbcn` (including junction gmin).
    base_collector: Dual,
    /// Substrate junction current (including junction gmin) and its derivative.
    substrate: (Real, Real),
    /// Base-resistance conductance `gx` and its bias derivatives (zero when
    /// `RB = 0`, which has no internal base node).
    base_conductance: Dual,
    /// Base-emitter charge `qbe(vbe, vbc)`.
    qbe: Dual,
    /// Intrinsic base-collector charge `qbc(vbc)` (derivative in `dvbc`).
    qbc: Dual,
    /// External base to internal collector charge `qbx(vbx)` and capacitance.
    qbx: (Real, Real),
    /// Substrate junction charge `qsub(vsub)` and capacitance.
    qsub: (Real, Real),
}

impl Evaluation {
    fn validate(&self) -> SpiceResult<()> {
        let duals = [
            self.transport,
            self.base_emitter,
            self.base_collector,
            self.base_conductance,
            self.qbe,
            self.qbc,
        ];
        let finite = duals
            .iter()
            .flat_map(|d| [d.value, d.dvbe, d.dvbc])
            .chain([
                self.substrate.0,
                self.substrate.1,
                self.qbx.0,
                self.qbx.1,
                self.qsub.0,
                self.qsub.1,
            ])
            .all(Real::is_finite);
        if !finite {
            return Err(SpiceError::Numerical {
                context: "BJT".into(),
                message: "nonfinite Gummel-Poon current, charge or Jacobian".into(),
            });
        }
        Ok(())
    }
}

/// The Gummel-Poon equations of `bjtload.c` at one bias, with analytic
/// derivatives. `gmin` is the junction gmin added to both base currents and
/// the substrate junction.
#[allow(clippy::too_many_lines)]
fn evaluate(t: &Thermal, bias: Bias, gmin: Real) -> SpiceResult<Evaluation> {
    let Bias {
        vbe,
        vbc,
        vbx,
        vsub,
    } = bias;
    let vt = t.vt;
    let (cbe, gbe) = pn(vbe, vt * t.nf, t.is_be);
    let (mut cben, mut gben) = if t.ise == 0. {
        (0., 0.)
    } else {
        pn(vbe, vt * t.ne, t.ise)
    };
    gben += gmin;
    cben += gmin * vbe;
    let (cbc, gbc) = pn(vbc, vt * t.nr, t.is_bc);
    let (mut cbcn, mut gbcn) = if t.isc == 0. {
        (0., 0.)
    } else {
        pn(vbc, vt * t.nc, t.isc)
    };
    gbcn += gmin;
    cbcn += gmin * vbc;
    let substrate = match t.iss {
        Some(iss) => {
            let vts = vt * t.ns;
            if vsub <= -3. * vts {
                let arg = (3. * vts / (vsub * std::f64::consts::E)).powi(3);
                (
                    -iss * (1. + arg) + gmin * vsub,
                    iss * 3. * arg / vsub + gmin,
                )
            } else {
                let e = (vsub / vts).min(MAX_EXP_ARG).exp();
                (iss * (e - 1.) + gmin * vsub, iss * e / vts + gmin)
            }
        }
        None => (gmin * vsub, gmin),
    };
    // Base charge qb (Early effect and high injection).
    let q1 = 1. / (1. - t.inv_vaf * vbc - t.inv_var * vbe);
    let (qb, dqbdve, dqbdvc) = if t.inv_ikf == 0. && t.inv_ikr == 0. {
        let qb = q1;
        (qb, q1 * qb * t.inv_var, q1 * qb * t.inv_vaf)
    } else {
        let q2 = t.inv_ikf * cbe + t.inv_ikr * cbc;
        let arg = (1. + 4. * q2).max(0.);
        match t.nkf {
            None => {
                let sqarg = if arg == 0. { 1. } else { arg.sqrt() };
                let qb = q1 * (1. + sqarg) / 2.;
                (
                    qb,
                    q1 * (qb * t.inv_var + t.inv_ikf * gbe / sqarg),
                    q1 * (qb * t.inv_vaf + t.inv_ikr * gbc / sqarg),
                )
            }
            Some(nkf) => {
                let sqarg = if arg == 0. { 1. } else { arg.powf(nkf) };
                let qb = q1 * (1. + sqarg) / 2.;
                (
                    qb,
                    q1 * (qb * t.inv_var + t.inv_ikf * gbe * 2. * sqarg * nkf / arg),
                    q1 * (qb * t.inv_vaf + t.inv_ikr * gbc * 2. * sqarg * nkf / arg),
                )
            }
        }
    };
    let transport = Dual {
        value: (cbe - cbc) / qb,
        dvbe: (gbe - (cbe - cbc) * dqbdve / qb) / qb,
        dvbc: (-gbc - (cbe - cbc) * dqbdvc / qb) / qb,
    };
    let base_emitter = Dual {
        value: cbe / t.bf + cben,
        dvbe: gbe / t.bf + gben,
        dvbc: 0.,
    };
    let base_collector = Dual {
        value: cbc / t.br + cbcn,
        dvbe: 0.,
        dvbc: gbc / t.br + gbcn,
    };
    let base_conductance = if t.rb == 0. && t.rbm == 0. {
        Dual::default()
    } else {
        base_conductance(t, qb, dqbdve, dqbdvc, base_emitter, base_collector)
    };
    // Base-emitter charge with the bias-dependent transit time.
    let (mut cbe_tf, mut gbe_tf, mut geqcb) = (cbe, gbe, 0.);
    if t.tf != 0. && vbe > 0. {
        let (mut argtf, mut arg2, mut arg3) = (0., 0., 0.);
        if t.xtf != 0. {
            argtf = t.xtf;
            if t.vtf_factor != 0. {
                argtf *= (vbc * t.vtf_factor).exp();
            }
            arg2 = argtf;
            if t.itf != 0. {
                let temp = cbe / (cbe + t.itf);
                argtf *= temp * temp;
                arg2 = argtf * (3. - temp - temp);
            }
            arg3 = cbe * argtf * t.vtf_factor;
        }
        cbe_tf = cbe * (1. + argtf) / qb;
        gbe_tf = (gbe * (1. + arg2) - cbe_tf * dqbdve) / qb;
        geqcb = t.tf * (arg3 - cbe_tf * dqbdvc) / qb;
    }
    let (qbe_dep, cbe_dep) = depletion(vbe, t.cje, t.vje, t.mje, t.fc_vje, t.f1, t.f2, t.f3);
    let qbe = Dual {
        value: t.tf * cbe_tf + qbe_dep,
        dvbe: t.tf * gbe_tf + cbe_dep,
        dvbc: geqcb,
    };
    let czbc = t.cjc * t.xcjc;
    let czbx = t.cjc - czbc;
    let (qbc_dep, cbc_dep) = depletion(vbc, czbc, t.vjc, t.mjc, t.fc_vjc, t.f5, t.f6, t.f7);
    let qbc = Dual {
        value: t.tr * cbc + qbc_dep,
        dvbe: 0.,
        dvbc: t.tr * gbc + cbc_dep,
    };
    let qbx = depletion(vbx, czbx, t.vjc, t.mjc, t.fc_vjc, t.f5, t.f6, t.f7);
    let qsub = if vsub < 0. {
        let arg = 1. - vsub / t.vjs;
        let sarg = (-t.mjs * arg.ln()).exp();
        (
            t.vjs * t.cjs * (1. - arg * sarg) / (1. - t.mjs),
            t.cjs * sarg,
        )
    } else {
        (
            vsub * t.cjs * (1. + t.mjs * vsub / (2. * t.vjs)),
            t.cjs * (1. + t.mjs * vsub / t.vjs),
        )
    };
    let evaluation = Evaluation {
        transport,
        base_emitter,
        base_collector,
        substrate,
        base_conductance,
        qbe,
        qbc,
        qbx,
        qsub,
    };
    evaluation.validate()?;
    Ok(evaluation)
}

/// `gx`, the base-resistance conductance of `bjtload.c`: `RBM + (RB-RBM)/qb`,
/// or the `IRB` current-crowding law, with its derivatives through `qb` or
/// the base current `cb`.
fn base_conductance(
    t: &Thermal,
    qb: Real,
    dqbdve: Real,
    dqbdvc: Real,
    base_emitter: Dual,
    base_collector: Dual,
) -> Dual {
    let rbpi = t.rb - t.rbm;
    let (resistance, dr_dvbe, dr_dvbc) = if t.irb == 0. {
        (
            t.rbm + rbpi / qb,
            -rbpi * dqbdve / (qb * qb),
            -rbpi * dqbdvc / (qb * qb),
        )
    } else {
        const K: Real = 14.59025;
        const C: Real = 2.4317;
        let cb = base_emitter.value + base_collector.value;
        let ratio = cb / t.irb;
        let (a, da_dvbe, da_dvbc) = if ratio > 1e-9 {
            (
                ratio,
                (base_emitter.dvbe + base_collector.dvbe) / t.irb,
                (base_emitter.dvbc + base_collector.dvbc) / t.irb,
            )
        } else {
            (1e-9, 0., 0.)
        };
        let root = (1. + K * a).sqrt();
        let z = (root - 1.) / C / a.sqrt();
        let dz_da = K / (2. * root * C * a.sqrt()) - z / (2. * a);
        let tz = z.tan();
        let numerator = tz - z;
        let denominator = z * tz * tz;
        let f = numerator / denominator;
        let sec2 = 1. + tz * tz;
        let df_dz = (tz * tz * denominator - numerator * (tz * tz + 2. * z * tz * sec2))
            / (denominator * denominator);
        let dr_da = 3. * rbpi * df_dz * dz_da;
        (t.rbm + 3. * rbpi * f, dr_da * da_dvbe, dr_da * da_dvbc)
    };
    if resistance == 0. {
        return Dual::default();
    }
    let g = 1. / resistance;
    Dual {
        value: g,
        dvbe: -g * g * dr_dvbe,
        dvbc: -g * g * dr_dvbc,
    }
}

/// The device's node roles. Primes are the internal (or, without the
/// corresponding resistance, external) nodes; `substrate_connection` is the
/// internal collector of a vertical device or the internal base of a lateral one.
#[derive(Debug, Clone, Copy)]
struct Nodes {
    c: NodeId,
    b: NodeId,
    e: NodeId,
    s: NodeId,
    cp: NodeId,
    bp: NodeId,
    ep: NodeId,
    sc: NodeId,
}

/// A current from `ends[0]` to `ends[1]` through the device, with its
/// partial derivatives with respect to node voltages.
struct Flow {
    ends: [NodeId; 2],
    value: Real,
    partials: Vec<(NodeId, Real)>,
    /// `sum(partial * dV)` for the difference between the (limited) junction
    /// voltages the flow was evaluated at and the solution's: the Newton
    /// linearization point is the evaluated one, as in `bjtload.c`.
    shift: Real,
}

/// A charge on `ends[0]` (and its negative on `ends[1]`) with partials.
struct Charge {
    slot: usize,
    ends: [NodeId; 2],
    value: Real,
    partials: Vec<(NodeId, Real)>,
    /// As [`Flow::shift`], for the charge.
    shift: Real,
}

/// Gummel-Poon level-1 BJT (`bjtload.c`), NPN or PNP.
#[derive(Debug)]
pub struct Bjt {
    name: String,
    /// The model card's name, for C's `.noise` visiting order.
    model_name: String,
    /// External terminals (three, or four with an explicit substrate) followed
    /// by the internal nodes, in the order `bjtsetup.c` creates them.
    terminals: Vec<NodeId>,
    nodes: Nodes,
    pol: Real,
    /// `SUBS`: +1 vertical, -1 lateral.
    subs: Real,
    model: Model,
    instance: Instance,
    /// `OFF` and `ICVBE`/`ICVCE` (C `BJToff`, `BJTicVBE`, `BJTicVCE`).
    initial: crate::devices::initial::InstanceInitial,
}

fn reject_unported(parameters: &[ParameterAssignment]) -> SpiceResult<()> {
    for parameter in parameters {
        if let Some((_, reference)) = UNPORTED_MODEL
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&parameter.name))
        {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "{}: BJT model parameter '{}'",
                    parameter.location, parameter.name
                ),
                format!("src/spicelib/devices/{reference}"),
            ));
        }
    }
    Ok(())
}

impl Bjt {
    pub(crate) fn instantiate(
        i: &DeviceInstance,
        nodes: &mut NodeTable,
        model: &ResolvedModel<'_>,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if model.levels().selector != 1 || !(3..=4).contains(&i.nodes.len()) {
            return Err(SpiceError::Unsupported {
                feature: "BJT backend requires level 1 and 3/4 terminals".into(),
                location: Some(i.location.clone()),
            });
        }
        let card = model.card();
        reject_unported(&card.parameters)?;
        let assignments: Vec<_> = card
            .parameters
            .iter()
            .filter(|p| !p.name.eq_ignore_ascii_case("level"))
            .map(canonical)
            .collect();
        let values = ScalarSchema { parameters: MODEL }.validate(&assignments, &card.location)?;
        let parameters = Model::from_values(&values)?;
        let excess_phase = get(&values, "ptf")? * parameters.tf;
        if excess_phase != 0. {
            return Err(SpiceError::not_yet_ported(
                format!("{}: BJT excess phase (PTF with TF != 0)", card.location),
                "src/spicelib/devices/bjt/bjtload.c (excess phase), bjtacld.c",
            ));
        }
        let (initial, instance_setters) =
            crate::devices::initial::split(&i.parameters, &IC_COMPONENTS)?;
        let instance_values = ScalarSchema {
            parameters: INSTANCE,
        }
        .validate(&instance_setters, &i.location)?;
        let area = get(&instance_values, "area")?;
        let instance = Instance {
            area,
            areab: given(&instance_values, "areab").unwrap_or(area),
            areac: given(&instance_values, "areac").unwrap_or(area),
            multiplier: get(&instance_values, "m")?,
            temp: given(&instance_values, "temp").map(|t| t + CELSIUS_TO_KELVIN),
            dtemp: get(&instance_values, "dtemp")?,
        };
        let pol = if model.family() == ModelFamily::Pnp {
            -1.
        } else {
            1.
        };
        // bjtsetup.c: SUBS other than +1/-1 takes the polarity default,
        // vertical for NPN and lateral for PNP.
        let subs = match given(&values, "subs") {
            Some(v) if v == 1. || v == -1. => v,
            _ => pol,
        };
        // Validate the temperature model and one evaluation before interning.
        let thermal = parameters.thermal(&instance, subs > 0., context)?;
        evaluate(
            &thermal,
            Bias {
                vbe: 0.,
                vbc: 0.,
                vbx: 0.,
                vsub: 0.,
            },
            context.gmin,
        )?;
        let mut staged = nodes.clone();
        let external: Vec<_> = i.nodes.iter().map(|n| staged.intern(n)).collect();
        let mut terminals = external.clone();
        let mut internal = |resistance: Real, suffix: &str, external: NodeId| {
            if resistance == 0. {
                return Ok(external);
            }
            let name = format!("{}#{suffix}", i.name);
            if staged.get(&name).is_some() {
                return Err(SpiceError::circuit(format!(
                    "BJT internal-node name collision: {name}"
                )));
            }
            let node = staged.intern(&name);
            staged.set_kind(node, NodeKind::Internal);
            terminals.push(node);
            Ok(node)
        };
        // bjtsetup.c creates collCX, base, emitter in this order.
        let cp = internal(parameters.rc, "collCX", external[0])?;
        let bp = internal(parameters.rb, "base", external[1])?;
        let ep = internal(parameters.re, "emitter", external[2])?;
        *nodes = staged;
        let nodes = Nodes {
            c: external[0],
            b: external[1],
            e: external[2],
            s: external.get(3).copied().unwrap_or(NodeId::GROUND),
            cp,
            bp,
            ep,
            sc: if subs > 0. { cp } else { bp },
        };
        Ok(Box::new(Self {
            name: i.name.clone(),
            model_name: card.name.clone(),
            terminals,
            nodes,
            pol,
            subs,
            model: parameters,
            instance,
            initial,
        }))
    }

    fn thermal(&self, context: &ModelContext) -> SpiceResult<Thermal> {
        self.model.thermal(&self.instance, self.subs > 0., context)
    }

    /// `bjtload.c`'s `uic` initial bias (`MODEINITJCT` under `MODETRANOP |
    /// MODEUIC`): `vbe = type * ICVBE`, `vbc = vbx = vbe - type * ICVCE`,
    /// `vsub = 0`. An unset `ICVBE`/`ICVCE` is the external base-emitter /
    /// collector-emitter voltage of the solution (`bjtgetic.c`).
    fn uic_bias(&self, voltage: impl Fn(NodeId) -> Real) -> Bias {
        let n = self.nodes;
        let icvbe = self.initial.values[0].unwrap_or_else(|| voltage(n.b) - voltage(n.e));
        let icvce = self.initial.values[1].unwrap_or_else(|| voltage(n.c) - voltage(n.e));
        let vbe = self.pol * icvbe;
        let vbc = vbe - self.pol * icvce;
        Bias {
            vbe,
            vbc,
            vbx: vbc,
            vsub: 0.,
        }
    }

    fn bias(&self, voltage: impl Fn(NodeId) -> Real) -> Bias {
        let n = self.nodes;
        Bias {
            vbe: self.pol * (voltage(n.bp) - voltage(n.ep)),
            vbc: self.pol * (voltage(n.bp) - voltage(n.cp)),
            vbx: self.pol * (voltage(n.b) - voltage(n.cp)),
            vsub: self.pol * self.subs * (voltage(n.s) - voltage(n.sc)),
        }
    }

    /// Physical terminal flows and charges, scaled by `m`. With
    /// `exact_base_resistance`, the base-resistance current's bias dependence
    /// enters the Jacobian (Newton); without, only `gx` (C's AC stamp).
    /// `shift` is the evaluated (limited) bias minus the solution's bias.
    fn flows(
        &self,
        e: &Evaluation,
        t: &Thermal,
        voltage: impl Fn(NodeId) -> Real,
        exact_base_resistance: bool,
        shift: Bias,
    ) -> (Vec<Flow>, Vec<Charge>) {
        let n = self.nodes;
        let m = self.instance.multiplier;
        let pol = self.pol;
        // Node differences b'-e', b'-c', s-sc are pol (* subs) times the
        // normalized voltages.
        let moved = |d: Dual| m * pol * (d.dvbe * shift.vbe + d.dvbc * shift.vbc);
        let moved_substrate = |g: Real| m * g * pol * self.subs * shift.vsub;
        // d(pol * f(vbe, vbc)) / dV over b', e', c'.
        let intrinsic = |d: Dual| {
            vec![
                (n.bp, m * (d.dvbe + d.dvbc)),
                (n.ep, -m * d.dvbe),
                (n.cp, -m * d.dvbc),
            ]
        };
        let mut flows = vec![
            Flow {
                ends: [n.cp, n.ep],
                value: m * pol * e.transport.value,
                partials: intrinsic(e.transport),
                shift: moved(e.transport),
            },
            Flow {
                ends: [n.bp, n.ep],
                value: m * pol * e.base_emitter.value,
                partials: intrinsic(e.base_emitter),
                shift: moved(e.base_emitter),
            },
            Flow {
                ends: [n.bp, n.cp],
                value: m * pol * e.base_collector.value,
                partials: intrinsic(e.base_collector),
                shift: moved(e.base_collector),
            },
            Flow {
                ends: [n.s, n.sc],
                value: m * pol * self.subs * e.substrate.0,
                partials: vec![(n.s, m * e.substrate.1), (n.sc, -m * e.substrate.1)],
                shift: moved_substrate(e.substrate.1),
            },
        ];
        if n.bp != n.b {
            let gx = e.base_conductance;
            let across = voltage(n.b) - voltage(n.bp);
            let mut partials = vec![(n.b, m * gx.value), (n.bp, -m * gx.value)];
            let mut moved_gx = 0.;
            if exact_base_resistance {
                // gx depends on pol*vbe and pol*vbc; the current does not
                // carry the polarity (it is an ordinary resistor current).
                let sensitivity = Dual {
                    value: 0.,
                    dvbe: pol * across * gx.dvbe,
                    dvbc: pol * across * gx.dvbc,
                };
                partials.extend(intrinsic(sensitivity));
                moved_gx = moved(sensitivity);
            }
            flows.push(Flow {
                ends: [n.b, n.bp],
                value: m * gx.value * across,
                partials,
                shift: moved_gx,
            });
        }
        for (ends, g) in [([n.c, n.cp], t.gc), ([n.e, n.ep], t.ge)] {
            if ends[0] != ends[1] {
                flows.push(Flow {
                    ends,
                    value: m * g * (voltage(ends[0]) - voltage(ends[1])),
                    partials: vec![(ends[0], m * g), (ends[1], -m * g)],
                    shift: 0.,
                });
            }
        }
        let charges = vec![
            Charge {
                slot: 0,
                ends: [n.bp, n.ep],
                value: m * pol * e.qbe.value,
                partials: intrinsic(e.qbe),
                shift: moved(e.qbe),
            },
            Charge {
                slot: 2,
                ends: [n.bp, n.cp],
                value: m * pol * e.qbc.value,
                partials: intrinsic(e.qbc),
                shift: moved(e.qbc),
            },
            Charge {
                slot: 4,
                ends: [n.s, n.sc],
                value: m * pol * self.subs * e.qsub.0,
                partials: vec![(n.s, m * e.qsub.1), (n.sc, -m * e.qsub.1)],
                shift: moved_substrate(e.qsub.1),
            },
            Charge {
                slot: 6,
                ends: [n.b, n.cp],
                value: m * pol * e.qbx.0,
                partials: vec![(n.b, m * e.qbx.1), (n.cp, -m * e.qbx.1)],
                shift: 0.,
            },
        ];
        (flows, charges)
    }
}

/// Stamps a flow's Newton linearization: Jacobian `partials` and the
/// equivalent current `value - sum(partial * V)`.
fn stamp_flow(context: &mut StampContext<'_>, flow: &Flow) -> SpiceResult<()> {
    let mut equivalent = flow.value - flow.shift;
    for (node, derivative) in &flow.partials {
        equivalent -= derivative * context.node_voltage(*node);
    }
    for (row, sign) in [(flow.ends[0], 1.), (flow.ends[1], -1.)] {
        for (col, derivative) in &flow.partials {
            context.stamp(row, *col, sign * derivative)?;
        }
        context.stamp_rhs(row, -sign * equivalent)?;
    }
    Ok(())
}

/// Integrates a charge with the trial companion coefficients (`NIintegrate`)
/// and stamps its current; outside transient only the charge is recorded.
/// `ag0 * dQ/dV` is the companion Jacobian, cross terms included.
fn stamp_charge(context: &mut StampContext<'_>, charge: &Charge) -> SpiceResult<()> {
    let slot = charge.slot;
    if let Some(coefficients) = context.integration {
        let crate::devices::AnalysisMode::Transient { dt, .. } = context.mode else {
            return Err(SpiceError::circuit(
                "BJT charge companion outside transient",
            ));
        };
        if dt != coefficients.dt() {
            return Err(SpiceError::circuit("BJT charge timestep mismatch"));
        }
        let mut history = vec![charge.value];
        for age in 1..=coefficients.charge_history_len() {
            history.push(
                context
                    .states
                    .accepted(age, slot)
                    .ok_or_else(|| SpiceError::circuit("missing accepted BJT charge"))?,
            );
        }
        let previous = if coefficients.needs_previous_derivative() {
            Some(
                context
                    .states
                    .accepted(1, slot + 1)
                    .ok_or_else(|| SpiceError::circuit("missing accepted BJT charge current"))?,
            )
        } else {
            None
        };
        // With unit capacitance the companion conductance is ag0 itself.
        let companion = coefficients.integrate(&history, previous, 1.)?;
        let ag0 = companion.conductance;
        stamp_flow(
            context,
            &Flow {
                ends: charge.ends,
                value: companion.derivative,
                partials: charge
                    .partials
                    .iter()
                    .map(|(node, dq)| (*node, ag0 * dq))
                    .collect(),
                shift: ag0 * charge.shift,
            },
        )?;
        context.states.set(slot + 1, companion.derivative)?;
    } else {
        if context.mode.is_transient() {
            return Err(SpiceError::circuit(
                "BJT transient needs companion integration",
            ));
        }
        context.states.set(slot + 1, 0.)?;
    }
    context.states.set(slot, charge.value)
}

/// Adds `partials` of a flow between `ends` into the AC conductance (`A`) or
/// charge (`E`) operator.
fn small_signal(
    context: &mut LinearContext<'_>,
    ends: [NodeId; 2],
    partials: &[(NodeId, Real)],
    dynamic: bool,
) -> SpiceResult<()> {
    for (row, sign) in [(ends[0], 1.), (ends[1], -1.)] {
        for (col, derivative) in partials {
            if let (Some(r), Some(c)) = (
                context.unknowns.node_row(row),
                context.unknowns.node_row(*col),
            ) {
                let matrix = if dynamic {
                    &mut context.system.e
                } else {
                    &mut context.system.a
                };
                matrix.add(r, c, sign * derivative)?;
            }
        }
    }
    Ok(())
}

impl Device for Bjt {
    fn name(&self) -> &str {
        &self.name
    }
    fn designator(&self) -> char {
        'q'
    }
    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }
    fn is_nonlinear(&self) -> bool {
        true
    }
    fn has_start_settings(&self) -> bool {
        self.initial.off
    }
    /// `qbe`, `qbc`, `qsub`, `qbx`, each followed by its current, then the
    /// limited `vbe`, `vbc`, `vsub`.
    fn state_count(&self) -> usize {
        11
    }
    /// `bjttrunc.c`: `qbe`, `qbc`, `qsub`, and `qbx` only when `XCJC < 1`.
    fn truncation_slots(&self) -> Vec<usize> {
        if self.model.xcjc < 1. {
            vec![0, 2, 4, 6]
        } else {
            vec![0, 2, 4]
        }
    }
    /// `bjt.c` `BJTpTable`: AREA, AREAB, AREAC, M, TEMP and DTEMP, which
    /// `bjttemp.c`/`bjtload.c` re-derive (`dctrcurv.c` `DCTsetInstParam`).
    fn instance_parameter(&self, keyword: &str) -> Option<&'static str> {
        ["area", "areab", "areac", "m", "temp", "dtemp"]
            .into_iter()
            .find(|name| name.eq_ignore_ascii_case(keyword))
    }
    /// `BJTparam` then `BJTtemp`. A swept AREA leaves AREAB/AREAC at the
    /// values `bjtsetup.c` defaulted them to from the card's AREA, exactly as
    /// C does: only `BJTsetup` copies AREA into an ungiven AREAB/AREAC.
    fn with_instance_parameter(
        &self,
        parameter: &str,
        value: Real,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        use crate::devices::sweep::check_swept;
        let mut instance = self.instance;
        let positive = || check_swept(&self.name, parameter, value, value > 0., "positive");
        match parameter {
            "area" => {
                positive()?;
                instance.area = value;
            }
            "areab" => {
                positive()?;
                instance.areab = value;
            }
            "areac" => {
                positive()?;
                instance.areac = value;
            }
            "m" => {
                positive()?;
                instance.multiplier = value;
            }
            "temp" => {
                check_swept(
                    &self.name,
                    parameter,
                    value,
                    value + CELSIUS_TO_KELVIN > 0.,
                    "above absolute zero",
                )?;
                instance.temp = Some(value + CELSIUS_TO_KELVIN);
            }
            "dtemp" => {
                check_swept(&self.name, parameter, value, true, "finite")?;
                instance.dtemp = value;
            }
            _ => {
                return Err(SpiceError::circuit(format!(
                    "{}: BJT parameter {parameter} cannot be swept",
                    self.name
                )));
            }
        }
        let device = Self {
            name: self.name.clone(),
            model_name: self.model_name.clone(),
            terminals: self.terminals.clone(),
            nodes: self.nodes,
            pol: self.pol,
            subs: self.subs,
            model: self.model,
            instance,
            initial: self.initial.clone(),
        };
        evaluate(
            &device.thermal(context)?,
            Bias {
                vbe: 0.,
                vbc: 0.,
                vbx: 0.,
                vsub: 0.,
            },
            context.gmin,
        )?;
        Ok(Box::new(device))
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit("BJT AC requires small-signal assembly"));
        }
        let model_context = context.model_context();
        let thermal = self.thermal(&model_context)?;
        let voltage = |node| context.node_voltage(node);
        let raw = self.bias(voltage);
        let mut limiter = Limiter::new(&context.states);
        let bias = thermal.limit(
            &mut limiter,
            &context.states,
            raw,
            self.uic_bias(voltage),
            self.initial.off,
        );
        let evaluation = evaluate(&thermal, bias, context.gmin)?;
        // BJTconvTest for an `off` instance held in MODEINITFIX: collector
        // `cc = it - ibc` and base `cb = ibe + ibc` currents at the held bias
        // against their linear prediction at the iterate's `vbe`, `vbc`.
        {
            let e = &evaluation;
            let (dbe, dbc) = (raw.vbe - bias.vbe, raw.vbc - bias.vbc);
            let predict = |d: [Dual; 2], sign: Real| {
                (d[0].dvbe + sign * d[1].dvbe) * dbe + (d[0].dvbc + sign * d[1].dvbc) * dbc
            };
            let cc = e.transport.value - e.base_collector.value;
            let cb = e.base_emitter.value + e.base_collector.value;
            limiter.test_held(
                &context.states,
                &[
                    (cc, cc + predict([e.transport, e.base_collector], -1.)),
                    (cb, cb + predict([e.base_emitter, e.base_collector], 1.)),
                ],
            );
        }
        let shift = Bias {
            vbe: bias.vbe - raw.vbe,
            vbc: bias.vbc - raw.vbc,
            vbx: 0.,
            vsub: bias.vsub - raw.vsub,
        };
        // The exact Jacobian unless the load asks for C's `bjtload.c` matrix
        // (gx only), as `.tf` does.
        let exact = !context.states.c_jacobian();
        let (flows, charges) = self.flows(&evaluation, &thermal, voltage, exact, shift);
        for flow in &flows {
            stamp_flow(context, flow)?;
        }
        for charge in &charges {
            stamp_charge(context, charge)?;
        }
        limiter.finish(
            &mut context.states,
            &[
                (BJT_LIMITED_SLOTS, bias.vbe),
                (BJT_LIMITED_SLOTS + 1, bias.vbc),
                (BJT_LIMITED_SLOTS + 2, bias.vsub),
            ],
        )
    }
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let thermal = self.thermal(context.model_context)?;
        let unknowns = context.unknowns;
        let voltage = |node: NodeId| {
            unknowns
                .node_row(node)
                .and_then(|r| bias.get(r))
                .unwrap_or(0.)
        };
        let evaluation = evaluate(&thermal, self.bias(voltage), context.model_context.gmin)?;
        // bjtacld.c stamps the base resistance as the conductance gx only.
        let (flows, charges) = self.flows(
            &evaluation,
            &thermal,
            voltage,
            false,
            Bias {
                vbe: 0.,
                vbc: 0.,
                vbx: 0.,
                vsub: 0.,
            },
        );
        for flow in &flows {
            small_signal(context, flow.ends, &flow.partials, false)?;
        }
        for charge in &charges {
            small_signal(context, charge.ends, &charge.partials, true)?;
        }
        Ok(())
    }

    /// Pole-zero load: C `bjtpzld.c` (excess phase is rejected at elaboration) equals the AC load with `s` for `j omega`.
    fn assemble_pole_zero(
        &self,
        context: &mut crate::devices::linear::LinearContext<'_>,
        bias: &crate::maths::Vector,
    ) -> crate::primitives::SpiceResult<()> {
        self.assemble_small_signal(context, bias)
    }

    /// `bjtnoise.c` at the operating point: thermal noise of RC, RB (the
    /// bias-dependent `gx`) and RE at the instance temperature, shot noise of
    /// the collector current `cc` and base current `cb` (C's `BJTcc`/`BJTcb`,
    /// junction gmin included) and the flicker law `m KF abs(cb)^AF / f`
    /// between the internal base and emitter. The quasi-saturation `_rci`
    /// generator is zero: RCO is not ported, so C's `collCX` node is the
    /// internal collector.
    fn noise(&self, context: &NoiseContext<'_>) -> SpiceResult<DeviceNoise> {
        let thermal = self.thermal(context.model_context)?;
        let e = evaluate(
            &thermal,
            self.bias(|node| context.voltage(node)),
            context.model_context.gmin,
        )?;
        let m = self.instance.multiplier;
        let cc = e.transport.value - e.base_collector.value;
        let cb = e.base_emitter.value + e.base_collector.value;
        let temperature = self
            .instance
            .temp
            .unwrap_or(context.model_context.temperature + CELSIUS_TO_KELVIN + self.instance.dtemp);
        let flicker = m * self.model.kf * (self.model.af * cb.abs().max(1e-38).ln()).exp();
        let n = self.nodes;
        let thermal_noise = |conductance: Real| NoiseKind::Thermal {
            conductance,
            temperature,
        };
        Ok(DeviceNoise::Sources {
            family: NoiseFamily::Bjt,
            model: Some(self.model_name.clone()),
            total: true,
            sources: vec![
                NoiseSource::new("_rc", [n.cp, n.c], thermal_noise(thermal.gc * m)),
                NoiseSource::new("_rci", [n.cp, n.cp], thermal_noise(0.)),
                NoiseSource::new(
                    "_rb",
                    [n.bp, n.b],
                    thermal_noise(e.base_conductance.value * m),
                ),
                NoiseSource::new("_re", [n.ep, n.e], thermal_noise(thermal.ge * m)),
                NoiseSource::new("_ic", [n.cp, n.ep], NoiseKind::Shot { current: cc * m }),
                NoiseSource::new("_ib", [n.bp, n.ep], NoiseKind::Shot { current: cb * m }),
                NoiseSource::new(
                    "_1overf",
                    [n.bp, n.ep],
                    NoiseKind::Flicker {
                        coefficient: flicker,
                        exponent: 1.,
                    },
                ),
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thermal(body: &str, temperature: Real, vertical: bool) -> Thermal {
        let deck = crate::netlist::source::parse_deck_text(
            std::path::Path::new("bjt.cir"),
            &format!("bjt\nq1 c b e qm\n.model qm npn({body})\n.end\n"),
        );
        let netlist = crate::netlist::Parser::new().parse_deck(&deck).unwrap();
        let card = &netlist.models[0];
        let assignments: Vec<_> = card.parameters.iter().map(canonical).collect();
        let values = ScalarSchema { parameters: MODEL }
            .validate(&assignments, &card.location)
            .unwrap();
        let model = Model::from_values(&values).unwrap();
        let instance = Instance {
            area: 1.3,
            areab: 1.3,
            areac: 1.3,
            multiplier: 1.,
            temp: None,
            dtemp: 0.,
        };
        model
            .thermal(&instance, vertical, &ModelContext::new(temperature, 27.))
            .unwrap()
    }

    type Pick = fn(&Evaluation) -> Dual;

    /// Every slice's analytic derivative matches a central finite difference.
    fn check_jacobian(t: &Thermal) {
        let gmin = 1e-12;
        let h = 1e-7;
        let biases = [
            (0.65, -2.),
            (0.75, -0.3),
            (0.8, 0.2),
            (0.85, 0.7),
            (-0.4, 0.62),
            (0.1, -0.05),
            (-1.5, -3.),
        ];
        for (vbe, vbc) in biases {
            let at = |vbe: Real, vbc: Real| {
                evaluate(
                    t,
                    Bias {
                        vbe,
                        vbc,
                        vbx: vbc,
                        vsub: -vbc,
                    },
                    gmin,
                )
                .unwrap()
            };
            let e = at(vbe, vbc);
            let pick: [(&str, Pick); 6] = [
                ("transport", |e| e.transport),
                ("base_emitter", |e| e.base_emitter),
                ("base_collector", |e| e.base_collector),
                ("gx", |e| e.base_conductance),
                ("qbe", |e| e.qbe),
                ("qbc", |e| e.qbc),
            ];
            for (name, f) in pick {
                let d = f(&e);
                let dvbe = (f(&at(vbe + h, vbc)).value - f(&at(vbe - h, vbc)).value) / (2. * h);
                let dvbc = (f(&at(vbe, vbc + h)).value - f(&at(vbe, vbc - h)).value) / (2. * h);
                for (analytic, numeric, what) in [(d.dvbe, dvbe, "vbe"), (d.dvbc, dvbc, "vbc")] {
                    let scale = analytic
                        .abs()
                        .max(numeric.abs())
                        .max(f(&e).value.abs() * 1e-3);
                    assert!(
                        (analytic - numeric).abs() <= 2e-5 * scale + 1e-14,
                        "{name} d/d{what} at ({vbe}, {vbc}): {analytic:e} vs {numeric:e}"
                    );
                }
            }
            // Single-voltage junctions: vbx (qbx) and vsub (substrate, qsub).
            let single = |v: Real, which: usize| {
                let e = evaluate(
                    t,
                    Bias {
                        vbe,
                        vbc,
                        vbx: v,
                        vsub: v,
                    },
                    gmin,
                )
                .unwrap();
                [e.qbx, e.substrate, e.qsub][which]
            };
            for which in 0..3 {
                for v in [vbc, -vbc, 0.3, -0.7] {
                    let (_, analytic) = single(v, which);
                    let numeric = (single(v + h, which).0 - single(v - h, which).0) / (2. * h);
                    assert!(
                        (analytic - numeric).abs()
                            <= 2e-5 * analytic.abs().max(numeric.abs()) + 1e-14,
                        "junction {which} at {v}: {analytic:e} vs {numeric:e}"
                    );
                }
            }
        }
    }

    #[test]
    fn gummel_poon_slices_have_exact_jacobians() {
        for body in [
            // Ebers-Moll core with charges.
            "is=1e-15 bf=80 br=2 cje=2p cjc=1p tf=0.3n tr=5n",
            // qb: Early and high injection, default sqrt and NKF exponent.
            "is=1e-15 vaf=40 var=8 ikf=3m ikr=0.5m",
            "is=1e-15 vaf=40 var=8 ikf=3m ikr=0.5m nkf=0.7",
            // Leakage, including the multiple-of-IS spelling.
            "is=1e-15 ise=1e-13 ne=1.6 isc=50 nc=1.8",
            // Base resistance through qb and through IRB.
            "is=1e-15 vaf=40 ikf=3m rb=200 rbm=20",
            "is=1e-15 ikf=3m rb=200 rbm=20 irb=50u",
            // Bias-dependent transit time.
            "is=1e-15 vaf=40 ikf=3m tf=0.4n xtf=3 vtf=2 itf=20m cje=2p mje=0.4 fc=0.6",
            "is=1e-15 tf=0.4n xtf=3",
            // Split base-collector charge and substrate junction.
            "is=1e-15 cjc=3p xcjc=0.4 mjc=0.5 cjs=2p mjs=0.3 vjs=0.6 iss=1e-17 ns=1.1",
        ] {
            check_jacobian(&thermal(body, 27., true));
            // Temperature scaling must keep the equations consistent too.
            check_jacobian(&thermal(body, 85., true));
        }
    }

    #[test]
    fn temperature_scaling_follows_bjttemp() {
        let nominal = thermal("is=1e-15 bf=100 xtb=1.5 cje=1p", 27., true);
        assert!((nominal.is_be - 1.3e-15).abs() < 1e-28);
        assert!((nominal.bf - 100.).abs() < 1e-12);
        let hot = thermal("is=1e-15 bf=100 xtb=1.5 cje=1p", 127., true);
        let (t, tnom) = (400.15, 300.15);
        let vt = K_OVER_Q * t;
        let factlog = (t / tnom - 1.) * 1.11 / vt + 3. * (t / tnom).ln();
        let expected = 1.3e-15 * factlog.exp();
        assert!((hot.is_be - expected).abs() < 1e-12 * expected);
        let bf = 100. * ((t / tnom).ln() * 1.5).exp();
        assert!((hot.bf - bf).abs() < 1e-12 * bf);
        // The built-in potential falls and the zero-bias capacitance rises.
        assert!(hot.vje < nominal.vje && hot.cje > nominal.cje);
        // TLEV=1 linear beta and TLEVC=1 linear capacitance/potential.
        let linear = thermal(
            "is=1e-15 bf=100 xtb=0.01 tlev=1 tlevc=1 cje=1p cte=1m vje=0.8 tvje=2m",
            127.,
            true,
        );
        assert!((linear.bf - 100. * (1. + 0.01 * 100.)).abs() < 1e-9);
        assert!((linear.cje - 1.3e-12 * 1.1).abs() < 1e-24);
        assert!((linear.vje - 0.6).abs() < 1e-12);
        // Polynomial resistance and Early-voltage coefficients.
        let poly = thermal("rb=100 trb1=1m rc=10 trc2=1e-5 vaf=50 tvaf1=-1m", 77., true);
        assert!((poly.rb - 100. * 1.05 / 1.3).abs() < 1e-9);
        assert!((poly.gc - 1.3 / (10. * 1.025)).abs() < 1e-12);
        assert!((poly.inv_vaf - 1. / (50. * 0.95)).abs() < 1e-15);
    }

    #[test]
    fn leakage_above_1e_minus_4_is_a_multiple_of_is() {
        let t = thermal("is=2e-15 ise=100 isc=1e-14", 27., true);
        assert!((t.ise - 1.3 * 2e-13).abs() < 1e-27);
        assert!((t.isc - 1.3e-14).abs() < 1e-27);
    }
}
