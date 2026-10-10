//! Junction equations, the level-1 diode and charge-aware trial stamping.
//!
//! C references: `dio/dioload.c`, `diosetup.c`, `diotemp.c`, `dioacld.c`,
//! `diompar.c`, `dioparam.c` and `NIintegrate`. The diode covers reverse
//! breakdown (BV/IBV/NBV/TCV), the full `diotemp.c` temperature laws (EG, XTI,
//! GAP1/GAP2, TLEV, TLEVC, DTEMP, TNOM), the sidewall junction (JSW, NS, CJSW,
//! VJSW, MJSW, FCS, PJ), recombination (ISR/NR), tunnelling (JTUN/JTUNSW/NTUN),
//! high-injection knees (IKF/IKR/IKP) and the transit-time/series-resistance
//! temperature coefficients, and the `.noise` generators of `dionoise.c`
//! (series-resistance thermal, junction shot and KF/AF flicker noise).
//! Soft reverse recovery, a separate sidewall series
//! resistance, self-heating, level-3 geometry and SOA setters fail
//! with [`SpiceError::NotYetPorted`] before node interning; unknown setters
//! remain unsupported errors. The instance `off` flag follows `dioload.c`
//! (`MODEINITJCT`/`MODEINITFIX` hold at 0 V). The `uic` initial load starts
//! the junction at the external terminal voltage of the `.ic`/`.nodeset`
//! node vector (`diogetic.c`); the instance `ic=` is accepted but has no
//! effect, because C's setter (`dioparam.c`) never sets `DIOinitCondGiven`
//! and `diogetic.c` therefore always overwrites it.
//!
//! The diode limits its junction voltage as `dioload.c` does (`MODEINITJCT`
//! start at `tVcrit`, `DEVpnjlim`, reflected about BV in breakdown) through
//! [`crate::devices::limiting`]; C's predictor and bypass are not ported.
use crate::devices::limiting::{self, Limiter, Linearization};
use crate::devices::noise::{DeviceNoise, NoiseContext, NoiseFamily, NoiseKind, NoiseSource};
use crate::devices::schema::{
    ScalarDomain as Domain, ScalarParameter as Parameter, ScalarSchema, ScalarUnit as Unit,
    ScalarValues,
};
use crate::devices::{Device, LinearContext, ModelContext, ResolvedModel, StampContext};
use crate::maths::Vector;
use crate::netlist::ast::{DeviceInstance, ParameterAssignment};
use crate::primitives::{NodeId, NodeKind, NodeTable, Real, SourceLoc, SpiceError, SpiceResult};

mod sens;

/// Diode state slot of the limited junction voltage (C `DIOvoltage`).
const DIODE_VOLTAGE_SLOT: usize = 2;
pub(crate) const K_OVER_Q: Real = 1.38064852e-23 / 1.6021766208e-19; // ngspice CONSTboltz/CHARGE
/// ngspice `CONSTboltz` (J/K).
const BOLTZMANN: Real = 1.38064852e-23;
/// ngspice `CHARGE` (C).
const CHARGE: Real = 1.6021766208e-19;
/// ngspice `REFTEMP`: 27 degrees Celsius in Kelvin.
const REFTEMP: Real = 300.15;
/// ngspice default `CKTepsmin` (`cktntask.c`): IS floor and knee-current cutoff.
const EPSMIN: Real = 1e-28;
/// C's breakdown matching stops once `|xcbv - cbv| <= CKTreltol * cbv`
/// (`diotemp.c`). Device temperature setup has no access to the run's RELTOL,
/// so the port applies ngspice's default RELTOL; a deck that changes `reltol`
/// can see C stop the (rapidly contracting) iteration one step earlier or later,
/// moving the knee voltage by at most that matching tolerance.
const BREAKDOWN_RELTOL: Real = 1e-3;

const fn scalar(
    name: &'static str,
    unit: Unit,
    domain: Domain,
    default: Option<Real>,
) -> Parameter {
    Parameter {
        name,
        unit,
        domain,
        default,
    }
}

/// The supported diode model setters, canonical names only (aliases are folded
/// by [`MODEL_ALIASES`] first). Defaults follow `diosetup.c`; setters without a
/// default are C "given" flags or context/model-dependent defaults.
const MODEL: ScalarSchema<'static> = ScalarSchema {
    parameters: &[
        scalar("is", Unit::Ampere, Domain::Positive, Some(1e-14)),
        scalar("jsw", Unit::Ampere, Domain::NonNegative, None),
        scalar("n", Unit::Dimensionless, Domain::Positive, Some(1.)),
        scalar("ns", Unit::Dimensionless, Domain::Positive, Some(1.)),
        scalar("rs", Unit::Ohm, Domain::NonNegative, Some(0.)),
        scalar("trs", Unit::InverseKelvin, Domain::Finite, Some(0.)),
        scalar("trs2", Unit::InverseKelvinSquared, Domain::Finite, Some(0.)),
        scalar("tnom", Unit::Celsius, Domain::Temperature, None),
        scalar("tt", Unit::Second, Domain::NonNegative, Some(0.)),
        scalar("ttt1", Unit::InverseKelvin, Domain::Finite, Some(0.)),
        scalar("ttt2", Unit::InverseKelvinSquared, Domain::Finite, Some(0.)),
        scalar("cjo", Unit::Farad, Domain::NonNegative, Some(0.)),
        scalar("vj", Unit::Volt, Domain::Positive, Some(1.)),
        scalar("m", Unit::Dimensionless, Domain::NonNegative, Some(0.5)),
        scalar("tm1", Unit::InverseKelvin, Domain::Finite, Some(0.)),
        scalar("tm2", Unit::InverseKelvinSquared, Domain::Finite, Some(0.)),
        scalar("fc", Unit::Dimensionless, Domain::NonNegative, Some(0.5)),
        scalar("cjsw", Unit::Farad, Domain::NonNegative, Some(0.)),
        scalar("vjsw", Unit::Volt, Domain::Positive, Some(1.)),
        scalar("mjsw", Unit::Dimensionless, Domain::NonNegative, Some(0.33)),
        scalar("fcs", Unit::Dimensionless, Domain::NonNegative, Some(0.5)),
        scalar("bv", Unit::Volt, Domain::Positive, None),
        scalar("ibv", Unit::Ampere, Domain::Positive, Some(1e-3)),
        scalar("nbv", Unit::Dimensionless, Domain::Positive, None),
        scalar("tcv", Unit::VoltPerKelvin, Domain::Finite, Some(0.)),
        scalar("tlev", Unit::Dimensionless, Domain::Finite, Some(0.)),
        scalar("tlevc", Unit::Dimensionless, Domain::Finite, Some(0.)),
        scalar("eg", Unit::ElectronVolt, Domain::Positive, None),
        scalar(
            "gap1",
            Unit::ElectronVoltPerKelvin,
            Domain::Finite,
            Some(7.02e-4),
        ),
        scalar("gap2", Unit::Kelvin, Domain::Finite, Some(1108.)),
        scalar("xti", Unit::Dimensionless, Domain::Finite, Some(3.)),
        scalar("cta", Unit::InverseKelvin, Domain::Finite, Some(0.)),
        scalar("ctp", Unit::InverseKelvin, Domain::Finite, Some(0.)),
        scalar("tpb", Unit::VoltPerKelvin, Domain::Finite, Some(0.)),
        scalar("tphp", Unit::VoltPerKelvin, Domain::Finite, Some(0.)),
        scalar("isr", Unit::Ampere, Domain::NonNegative, None),
        scalar("nr", Unit::Dimensionless, Domain::Positive, Some(2.)),
        scalar("ikf", Unit::Ampere, Domain::Finite, None),
        scalar("ikr", Unit::Ampere, Domain::Finite, None),
        scalar("ikp", Unit::Ampere, Domain::Finite, None),
        scalar("jtun", Unit::Ampere, Domain::NonNegative, None),
        scalar("jtunsw", Unit::Ampere, Domain::NonNegative, None),
        scalar("ntun", Unit::Dimensionless, Domain::Positive, Some(30.)),
        scalar("xtitun", Unit::Dimensionless, Domain::Finite, Some(3.)),
        scalar("keg", Unit::Dimensionless, Domain::Finite, Some(1.)),
        scalar("area", Unit::Dimensionless, Domain::Positive, Some(1.)),
        scalar("pj", Unit::Dimensionless, Domain::NonNegative, Some(0.)),
        // dionoise.c flicker law; diosetup.c defaults KF = 0, AF = 1.
        scalar("kf", Unit::Dimensionless, Domain::Finite, Some(0.)),
        scalar("af", Unit::Dimensionless, Domain::Finite, Some(1.)),
    ],
};
/// `dio.c::DIOmPTable` aliases (`IOPR` entries share the canonical setter id, so
/// last-set precedence spans the alias and its canonical name).
const MODEL_ALIASES: &[(&str, &str)] = &[
    ("js", "is"),
    ("isw", "jsw"),
    ("tref", "tnom"),
    ("trs1", "trs"),
    ("cj0", "cjo"),
    ("cj", "cjo"),
    ("pb", "vj"),
    ("mj", "m"),
    ("cjp", "cjsw"),
    ("php", "vjsw"),
    ("ik", "ikf"),
    ("nz", "nbv"),
    ("vb", "bv"),
    ("vrb", "bv"),
    ("var", "bv"),
    ("ib", "ibv"),
    ("tbv1", "tcv"),
    ("ctc", "cta"),
    ("tvj", "tpb"),
];
/// Recognised C model setters whose physics is not ported yet.
const MODEL_PENDING: &[(&str, &str)] = &[
    (
        "rsw",
        "src/spicelib/devices/dio/diosetup.c, dioload.c (separate sidewall series resistance)",
    ),
    (
        "vp",
        "src/spicelib/devices/dio/dioload.c, diotemp.c (soft reverse recovery)",
    ),
    (
        "qpscale",
        "src/spicelib/devices/dio/dioload.c, diotemp.c (soft reverse recovery)",
    ),
    ("rth0", "src/spicelib/devices/dio/dioload.c (self-heating)"),
    ("cth0", "src/spicelib/devices/dio/dioload.c (self-heating)"),
    ("fv_max", "src/spicelib/devices/dio/diosoachk.c"),
    ("bv_max", "src/spicelib/devices/dio/diosoachk.c"),
    ("id_max", "src/spicelib/devices/dio/diosoachk.c"),
    ("te_max", "src/spicelib/devices/dio/diosoachk.c"),
    ("pd_max", "src/spicelib/devices/dio/diosoachk.c"),
    (
        "lm",
        "src/spicelib/devices/dio/diosetup.c (level 3 geometry)",
    ),
    (
        "lp",
        "src/spicelib/devices/dio/diosetup.c (level 3 geometry)",
    ),
    (
        "wm",
        "src/spicelib/devices/dio/diosetup.c (level 3 geometry)",
    ),
    (
        "wp",
        "src/spicelib/devices/dio/diosetup.c (level 3 geometry)",
    ),
    (
        "xom",
        "src/spicelib/devices/dio/diosetup.c (level 3 geometry)",
    ),
    (
        "xoi",
        "src/spicelib/devices/dio/diosetup.c (level 3 geometry)",
    ),
    (
        "xm",
        "src/spicelib/devices/dio/diosetup.c (level 3 geometry)",
    ),
    (
        "xp",
        "src/spicelib/devices/dio/diosetup.c (level 3 geometry)",
    ),
    (
        "xw",
        "src/spicelib/devices/dio/diosetup.c (level 3 geometry)",
    ),
];
const INSTANCE: ScalarSchema<'static> = ScalarSchema {
    parameters: &[
        scalar("area", Unit::Dimensionless, Domain::Positive, None),
        scalar("pj", Unit::Dimensionless, Domain::NonNegative, None),
        scalar("m", Unit::Dimensionless, Domain::Positive, Some(1.)),
        scalar("temp", Unit::Celsius, Domain::Temperature, None),
        scalar("dtemp", Unit::Kelvin, Domain::Finite, None),
    ],
};
/// Recognised C instance setters whose behaviour is not ported yet.
const INSTANCE_PENDING: &[(&str, &str)] = &[
    (
        "thermal",
        "src/spicelib/devices/dio/dioload.c (self-heating)",
    ),
    (
        "w",
        "src/spicelib/devices/dio/diosetup.c (level 3 geometry)",
    ),
    (
        "l",
        "src/spicelib/devices/dio/diosetup.c (level 3 geometry)",
    ),
    (
        "lm",
        "src/spicelib/devices/dio/diosetup.c (level 3 geometry)",
    ),
    (
        "lp",
        "src/spicelib/devices/dio/diosetup.c (level 3 geometry)",
    ),
    (
        "wm",
        "src/spicelib/devices/dio/diosetup.c (level 3 geometry)",
    ),
    (
        "wp",
        "src/spicelib/devices/dio/diosetup.c (level 3 geometry)",
    ),
];

pub(crate) fn value(values: &ScalarValues, name: &str) -> SpiceResult<Real> {
    values
        .get(name)
        .map(|v| v.value)
        .ok_or_else(|| SpiceError::circuit(format!("missing nonlinear schema default {name}")))
}
/// An explicitly written setter (a schema default has no location).
fn given(values: &ScalarValues, name: &str) -> Option<Real> {
    values
        .get(name)
        .filter(|v| v.location.is_some())
        .map(|v| v.value)
}
/// Reject recognised-but-unported setters with their C reference, then fold
/// aliases onto canonical names in their original order.
fn canonical<'a>(
    assignments: impl IntoIterator<Item = &'a ParameterAssignment>,
    aliases: &[(&str, &str)],
    pending: &[(&str, &str)],
    owner: &str,
) -> SpiceResult<Vec<ParameterAssignment>> {
    let mut out = Vec::new();
    for assignment in assignments {
        let name = assignment.name.to_ascii_lowercase();
        if let Some((_, reference)) = pending.iter().find(|(p, _)| *p == name) {
            return Err(SpiceError::not_yet_ported(
                format!("{}: diode {owner} setter '{name}'", assignment.location),
                *reference,
            ));
        }
        let mut assignment = assignment.clone();
        if let Some((_, target)) = aliases.iter().find(|(alias, _)| *alias == name) {
            (*target).clone_into(&mut assignment.name);
        }
        out.push(assignment);
    }
    Ok(out)
}
/// `floor(raw + 0.5)` integer selector (`inpgval.c` IF_INTEGER) within `0..=max`.
fn selector(values: &ScalarValues, name: &str, max: u8, owner: &DeviceInstance) -> SpiceResult<u8> {
    let raw = value(values, name)?;
    let rounded = (raw + 0.5).floor();
    if (0. ..=Real::from(max)).contains(&rounded) {
        // In range 0..=max, so the cast is exact.
        Ok(rounded as u8)
    } else {
        Err(SpiceError::parse(
            values
                .get(name)
                .and_then(|v| v.location.clone())
                .unwrap_or_else(|| owner.location.clone()),
            format!("diode {name} must round to 0..={max}, got {raw}"),
        ))
    }
}

/// A level-1 diode with an optional internal anode for series resistance.
/// Only the explicitly enumerated diode model/instance schema is supported.
#[derive(Debug)]
pub struct Diode {
    name: String,
    /// The model card's name, for C's `.noise` visiting order.
    model: String,
    terminals: Vec<NodeId>,
    junction: [NodeId; 2],
    parameters: DiodeParameters,
    /// `off` and the instance `ic` (C `DIOoff`, `DIOinitCond`). The `ic`
    /// value is validated and kept but, exactly as in C, never used: see
    /// the `uic` start in `stamp`.
    initial: crate::devices::initial::InstanceInitial,
    /// The instance card, for diagnostics of swept replacements.
    location: SourceLoc,
    /// The validated model and instance setters as written, for `.sens`
    /// ([`sens`]), shared by swept and perturbed copies.
    written: std::rc::Rc<(ScalarValues, ScalarValues)>,
    /// A `.sens` stand-in: the load keeps C's nonfinite values (a knee
    /// current of zero divides by zero in `dioload.c`) instead of rejecting
    /// them, so the analysis can propagate C's NaN.
    lenient: bool,
}
/// Typed validated diode parameters as written (nominal temperature values,
/// instance scale factors kept separate). Temperature-dependent quantities are
/// derived per evaluation by [`DiodeParameters::thermal`], never cumulatively.
#[derive(Debug, Clone, Copy)]
struct DiodeParameters {
    is: Real,
    /// JSW; `Some` (even zero) enables the sidewall current (C `DIOsatSWCurGiven`).
    jsw: Option<Real>,
    n: Real,
    ns: Real,
    /// NS given: the sidewall current has its own characteristic.
    ns_given: bool,
    rs: Real,
    trs1: Real,
    trs2: Real,
    tt: Real,
    ttt1: Real,
    ttt2: Real,
    cjo: Real,
    vj: Real,
    grading: Real,
    tm1: Real,
    tm2: Real,
    fc: Real,
    cjsw: Real,
    vjsw: Real,
    mjsw: Real,
    fcs: Real,
    bv: Option<Real>,
    ibv: Real,
    nbv: Real,
    tcv: Real,
    tlev: u8,
    tlevc: u8,
    eg: Real,
    gap1: Real,
    gap2: Real,
    xti: Real,
    cta: Real,
    ctp: Real,
    tpb: Real,
    tphp: Real,
    isr: Option<Real>,
    nr: Real,
    ikf: Option<Real>,
    ikr: Option<Real>,
    ikp: Option<Real>,
    jtun: Option<Real>,
    jtunsw: Option<Real>,
    ntun: Real,
    xtitun: Real,
    keg: Real,
    area: Real,
    perimeter: Real,
    multiplier: Real,
    temperature: Option<Real>,
    dtemp: Real,
    nominal: Option<Real>,
    /// Flicker-noise coefficient and exponent (`dionoise.c`).
    kf: Real,
    af: Real,
}
impl Diode {
    pub(crate) fn instantiate(
        instance: &DeviceInstance,
        nodes: &mut NodeTable,
        model: &ResolvedModel<'_>,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if instance.nodes.len() != 2 {
            return Err(SpiceError::circuit("diode needs two terminals"));
        }
        let card = model.card();
        let model_setters = canonical(
            card.parameters
                .iter()
                .filter(|p| !p.name.eq_ignore_ascii_case("level")),
            MODEL_ALIASES,
            MODEL_PENDING,
            "model",
        )?;
        let m = MODEL.validate(&model_setters, &card.location)?;
        let (initial, setters) = crate::devices::initial::split(&instance.parameters, &["ic"])?;
        let instance_setters = canonical(&setters, &[], INSTANCE_PENDING, "instance")?;
        let i = INSTANCE.validate(&instance_setters, &instance.location)?;
        let p = DiodeParameters::new(&m, &i, instance)?;
        p.thermal(context)?.point(0., context.gmin)?; // validate before interning
        let mut staged = nodes.clone();
        let external = [
            staged.intern(&instance.nodes[0]),
            staged.intern(&instance.nodes[1]),
        ];
        let mut terminals = external.to_vec();
        let positive = if p.rs > 0. {
            let name = format!("{}#anode", instance.name);
            if staged.get(&name).is_some() {
                return Err(SpiceError::circuit("diode internal-node name collision"));
            }
            let prime = staged.intern(&name);
            staged.set_kind(prime, NodeKind::Internal);
            terminals.push(prime);
            prime
        } else {
            external[0]
        };
        *nodes = staged;
        Ok(Box::new(Self {
            name: instance.name.clone(),
            model: card.name.clone(),
            terminals,
            junction: [positive, external[1]],
            parameters: p,
            initial,
            location: instance.location.clone(),
            written: std::rc::Rc::new((m, i)),
            lenient: false,
        }))
    }
}
impl DiodeParameters {
    fn new(m: &ScalarValues, i: &ScalarValues, instance: &DeviceInstance) -> SpiceResult<Self> {
        let tlev = selector(m, "tlev", 2, instance)?;
        let n = value(m, "n")?;
        // diompar.c: IKF/IKR/IKP below CKTepsmin disable the effect.
        let knee = |name| given(m, name).filter(|&k| k >= EPSMIN);
        let p = Self {
            // diosetup.c: IS is floored at CKTepsmin.
            is: value(m, "is")?.max(EPSMIN),
            jsw: given(m, "jsw"),
            n,
            ns: value(m, "ns")?,
            ns_given: given(m, "ns").is_some(),
            rs: value(m, "rs")?,
            trs1: value(m, "trs")?,
            trs2: value(m, "trs2")?,
            tt: value(m, "tt")?,
            ttt1: value(m, "ttt1")?,
            ttt2: value(m, "ttt2")?,
            cjo: value(m, "cjo")?,
            vj: value(m, "vj")?,
            grading: value(m, "m")?,
            tm1: value(m, "tm1")?,
            tm2: value(m, "tm2")?,
            fc: value(m, "fc")?,
            cjsw: value(m, "cjsw")?,
            vjsw: value(m, "vjsw")?,
            mjsw: value(m, "mjsw")?,
            fcs: value(m, "fcs")?,
            bv: given(m, "bv"),
            ibv: value(m, "ibv")?,
            nbv: given(m, "nbv").unwrap_or(n),
            tcv: value(m, "tcv")?,
            tlev,
            tlevc: selector(m, "tlevc", 1, instance)?,
            eg: given(m, "eg").unwrap_or(if tlev == 2 { 1.16 } else { 1.11 }),
            gap1: value(m, "gap1")?,
            gap2: value(m, "gap2")?,
            xti: value(m, "xti")?,
            cta: value(m, "cta")?,
            ctp: value(m, "ctp")?,
            tpb: value(m, "tpb")?,
            tphp: value(m, "tphp")?,
            isr: given(m, "isr"),
            nr: value(m, "nr")?,
            ikf: knee("ikf"),
            ikr: knee("ikr"),
            ikp: knee("ikp"),
            jtun: given(m, "jtun"),
            jtunsw: given(m, "jtunsw"),
            ntun: value(m, "ntun")?,
            xtitun: value(m, "xtitun")?,
            keg: value(m, "keg")?,
            // diosetup.c: instance AREA/PJ default to the model's.
            area: i.get("area").map_or(value(m, "area")?, |v| v.value),
            perimeter: i.get("pj").map_or(value(m, "pj")?, |v| v.value),
            multiplier: value(i, "m")?,
            temperature: i.get("temp").map(|v| v.value),
            dtemp: i.get("dtemp").map_or(0., |v| v.value),
            nominal: m.get("tnom").map(|v| v.value),
            kf: value(m, "kf")?,
            af: value(m, "af")?,
        };
        p.check(&instance.location)?;
        if p.temperature.is_some() && i.get("dtemp").is_some() {
            return Err(SpiceError::parse(
                instance.location.clone(),
                "diode has both temp= and dtemp= (C ignores dtemp); give one",
            ));
        }
        Ok(p)
    }

    /// The instance-independent consistency checks of [`Self::new`], shared
    /// with swept replacements ([`Diode::with_instance_parameter`]).
    fn check(&self, location: &SourceLoc) -> SpiceResult<()> {
        let p = self;
        let location = || location.clone();
        if p.grading >= 1. || p.fc >= 1. || p.mjsw >= 1. || p.fcs >= 1. {
            return Err(SpiceError::parse(
                location(),
                "diode requires 0 <= M,FC,MJSW,FCS < 1",
            ));
        }
        if !(p.area * p.multiplier).is_finite() || !(p.perimeter * p.multiplier).is_finite() {
            return Err(SpiceError::parse(
                location(),
                "diode needs finite area*m, pj*m",
            ));
        }
        // dioload.c evaluates the common-characteristic sidewall breakdown with
        // `vdsw`, which is only assigned for a separate sidewall (RSW), so C's
        // value there depends on stale solver state rather than on the junction.
        if p.bv.is_some() && !p.ns_given && p.jsw.unwrap_or(0.) * p.perimeter > 0. {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "{}: diode sidewall current (JSW*PJ > 0) sharing the bottom characteristic \
                     (NS not given) in breakdown (BV given)",
                    location()
                ),
                "src/spicelib/devices/dio/dioload.c (common-characteristic sidewall breakdown)",
            ));
        }
        // dioload.c/diotemp.c mix the temperature-adjusted (F1SW) and nominal
        // (F2SW, F3SW, charge below FCS*VJSW) sidewall grading under TM1/TM2.
        if p.cjsw * p.perimeter > 0. && (p.tm1 != 0. || p.tm2 != 0.) {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "{}: diode sidewall depletion charge with grading temperature \
                     coefficients TM1/TM2",
                    location()
                ),
                "src/spicelib/devices/dio/diotemp.c, dioload.c (DIOtGradingCoeffSW)",
            ));
        }
        Ok(())
    }

    /// `diotemp.c::DIOtempUpdate` at this run's instance temperature.
    fn thermal(&self, context: &ModelContext) -> SpiceResult<Thermal> {
        // diotemp.c::DIOtemp: TEMP, else circuit temperature plus DTEMP.
        let t = self.temperature.unwrap_or(context.temperature + self.dtemp) + 273.15;
        let tn = self.nominal.unwrap_or(context.nominal_temperature) + 273.15;
        if !t.is_finite() || t <= 0. || !tn.is_finite() || tn <= 0. {
            return Err(SpiceError::circuit("invalid diode temperature"));
        }
        let vt = K_OVER_Q * t;
        let vtnom = K_OVER_Q * tn;
        let dt = t - tn;
        let ln_ratio = (t / tn).ln();
        let scale = self.area * self.multiplier;
        let perimeter = self.perimeter * self.multiplier;

        let factor = 1. + self.tm1 * dt + self.tm2 * dt * dt;
        let grading = self.grading * factor;

        // Band gap at T and TNOM: the fixed silicon law for TLEV 0/1, the
        // EG/GAP1/GAP2 law for TLEV 2.
        let (egfet, egfet1) = if self.tlev == 2 {
            (
                self.eg - self.gap1 * t * t / (t + self.gap2),
                self.eg - self.gap1 * tn * tn / (tn + self.gap2),
            )
        } else {
            (
                1.16 - 7.02e-4 * t * t / (t + 1108.),
                1.16 - 7.02e-4 * tn * tn / (tn + 1108.),
            )
        };
        let fact2 = t / REFTEMP;
        let arg = -egfet / (2. * BOLTZMANN * t) + 1.1150877 / (BOLTZMANN * (REFTEMP + REFTEMP));
        let pbfact = -2. * vt * (1.5 * fact2.ln() + CHARGE * arg);
        let arg1 = -egfet1 / (BOLTZMANN * 2. * tn) + 1.1150877 / (2. * BOLTZMANN * REFTEMP);
        let fact1 = tn / REFTEMP;
        let pbfact1 = -2. * vtnom * (1.5 * fact1.ln() + CHARGE * arg1);
        // Depletion capacitance/potential temperature laws (TLEVC 0 or 1).
        let depletion = |cap: Real, pot: Real, grading: Real, ct: Real, tp: Real| {
            if self.tlevc == 0 {
                let pbo = (pot - pbfact1) / fact1;
                let gmaold = (pot - pbo) / pbo;
                let mut c = cap / (1. + grading * (400e-6 * (tn - REFTEMP) - gmaold));
                let pot_t = pbfact + fact2 * pbo;
                let gmanew = (pot_t - pbo) / pbo;
                c *= 1. + grading * (400e-6 * (t - REFTEMP) - gmanew);
                (c, pot_t)
            } else {
                (cap * (1. + ct * (t - REFTEMP)), pot - tp * (t - REFTEMP))
            }
        };
        let (cjo, vj) = depletion(self.cjo * scale, self.vj, grading, self.cta, self.tpb);
        let (cjsw, vjsw) = depletion(
            self.cjsw * perimeter,
            self.vjsw,
            self.mjsw,
            self.ctp,
            self.tphp,
        );

        // Saturation currents: `n` is the emission coefficient, `xti` the
        // exponent and `k` the band-gap factor (KEG for tunnelling).
        let saturation = |base: Real, n: Real, xti: Real, k: Real| {
            let vte = n * vt;
            let exponent = if self.tlev == 2 {
                k * egfet1 / (n * vtnom) - k * egfet / vte + xti / n * ln_ratio
            } else {
                (t / tn - 1.) * k * self.eg / vte + xti / n * ln_ratio
            };
            base * exponent.exp()
        };
        let csat = saturation(self.is * scale, self.n, self.xti, 1.);
        let csatsw = saturation(self.jsw.unwrap_or(0.) * perimeter, self.ns, self.xti, 1.);
        let tunnel = |j: Option<Real>, s: Real| {
            j.map(|j| saturation(j * s, self.ntun, self.xtitun, self.keg))
        };
        let breakdown = match self.bv {
            Some(bv) => Some(breakdown_voltage(
                if self.tlev == 0 {
                    bv - self.tcv * dt
                } else {
                    bv * (1. - self.tcv * dt)
                },
                // Level 1: IBV scales with M only, not AREA.
                self.multiplier * self.ibv,
                csat + csatsw,
                vt,
                self.nbv,
            )?),
            None => None,
        };
        let polynomial = |a: Real, b: Real| 1. + a * dt + b * dt * dt;
        let conductance = if self.rs > 0. {
            let factor = polynomial(self.trs1, self.trs2);
            if factor <= 0. || factor.is_nan() {
                return Err(SpiceError::Numerical {
                    context: "diode".into(),
                    message: "nonpositive temperature-adjusted series resistance".into(),
                });
            }
            scale / self.rs / factor
        } else {
            0.
        };
        let thermal = Thermal {
            vt,
            n: self.n,
            ns: self.ns_given.then_some(self.ns),
            nbv: self.nbv,
            csat,
            csatsw: self.jsw.map(|_| csatsw),
            breakdown,
            recombination: self
                .isr
                .map(|isr| (saturation(isr * scale, self.nr, self.xti, 1.), self.nr)),
            tunnel: tunnel(self.jtun, scale),
            tunnel_sidewall: tunnel(self.jtunsw, perimeter),
            ntun: self.ntun,
            ikf: self.ikf.map(|k| k * scale),
            ikr: self.ikr.map(|k| k * scale),
            ikp: self.ikp.map(|k| k * perimeter),
            cjo,
            vj,
            grading,
            fc: self.fc,
            cjsw,
            vjsw,
            mjsw: self.mjsw,
            fcs: self.fcs,
            // diotemp.c: tt must stay positive.
            tt: self.tt * polynomial(self.ttt1, self.ttt2).max(1e-3),
            conductance,
        };
        thermal.validate()?;
        Ok(thermal)
    }
}

/// `diotemp.c` breakdown matching: the knee voltage `xbv` at which the
/// reverse-breakdown exponential carries `cbv` at the temperature-adjusted
/// breakdown voltage `bv`, iterated like C (see [`BREAKDOWN_RELTOL`]).
fn breakdown_voltage(bv: Real, cbv: Real, total: Real, vt: Real, nbv: Real) -> SpiceResult<Real> {
    if cbv < total * bv / vt {
        // C keeps BV unmatched (its warning is TRACE-only).
        return Ok(bv);
    }
    let matched = |x: Real| total * (((bv - x) / (nbv * vt)).exp() - 1. + x / vt);
    let tolerance = BREAKDOWN_RELTOL * cbv;
    let mut x = bv - nbv * vt * (1. + cbv / total).ln();
    for _ in 0..25 {
        x = bv - nbv * vt * (cbv / total + 1. - x / vt).ln();
        if (matched(x) - cbv).abs() <= tolerance {
            break;
        }
    }
    // After 25 iterations C keeps the last iterate (its warning is TRACE-only).
    if !x.is_finite() {
        return Err(SpiceError::Numerical {
            context: "diode".into(),
            message: "nonfinite breakdown matching".into(),
        });
    }
    Ok(x)
}

/// Temperature-adjusted, instance-scaled diode quantities (`diotemp.c` outputs).
#[derive(Debug, Clone, Copy)]
struct Thermal {
    vt: Real,
    n: Real,
    /// Sidewall emission coefficient when NS is given (own characteristic).
    ns: Option<Real>,
    nbv: Real,
    csat: Real,
    /// Sidewall saturation current when JSW is given.
    csatsw: Option<Real>,
    /// Matched breakdown voltage when BV is given.
    breakdown: Option<Real>,
    /// Recombination saturation current and NR when ISR is given.
    recombination: Option<(Real, Real)>,
    tunnel: Option<Real>,
    tunnel_sidewall: Option<Real>,
    ntun: Real,
    ikf: Option<Real>,
    ikr: Option<Real>,
    ikp: Option<Real>,
    cjo: Real,
    vj: Real,
    grading: Real,
    fc: Real,
    cjsw: Real,
    vjsw: Real,
    mjsw: Real,
    fcs: Real,
    tt: Real,
    /// Series conductance AREA*M/RS at temperature (0 without RS).
    conductance: Real,
}
/// One diode evaluation. The Newton current/conductance and charge/capacitance
/// are exact derivatives; `ac_*` are C's small-signal values (they differ only
/// with ISR, see [`Thermal::point`]).
#[derive(Debug, Clone, Copy)]
struct DiodePoint {
    junction: JunctionPoint,
    ac_conductance: Real,
    ac_capacitance: Real,
}
impl Thermal {
    /// `dioload.c`'s junction voltage for this load: `tVcrit` in
    /// `MODEINITJCT`, otherwise `DEVpnjlim` against the previous value, in
    /// the breakdown frame (`-(vd + BV)` with `nbv * vt`) when BV is given
    /// and `vd < min(0, -BV + 10 nbv vt)`. `tVcrit` uses the total (bottom
    /// plus sidewall) saturation current, as `diotemp.c` does without RSW.
    ///
    /// `start` is C's `uic` initial voltage (`DIOinitCond`) and `off` the
    /// instance flag: the `uic` initial load evaluates at `start`, and an
    /// `off` diode is held at zero in `MODEINITJCT` and `MODEINITFIX`.
    fn limit(
        &self,
        limiter: &mut Limiter,
        states: &crate::devices::DeviceState<'_>,
        raw: Real,
        start: Real,
        off: bool,
    ) -> Real {
        let vte = self.n * self.vt;
        let vcrit = limiting::critical_voltage(vte, self.csat + self.csatsw.unwrap_or(0.));
        if limiter.mode() == Linearization::InitialConditions {
            return start;
        }
        if limiter.holds_off(states, off) {
            return 0.;
        }
        if limiter.mode() == Linearization::Initial {
            return vcrit;
        }
        let previous = limiter.previous(states, DIODE_VOLTAGE_SLOT);
        let vtebrk = self.nbv * self.vt;
        match self.breakdown {
            Some(bv) if raw < (-bv + 10. * vtebrk).min(0.) => {
                let reflected = limiter.pn_junction(
                    -(raw + bv),
                    previous.map(|old| -(old + bv)),
                    vtebrk,
                    vcrit,
                );
                -(reflected + bv)
            }
            _ => limiter.pn_junction(raw, previous, vte, vcrit),
        }
    }

    fn validate(&self) -> SpiceResult<()> {
        let finite = [
            self.csat,
            self.csatsw.unwrap_or(0.),
            self.breakdown.unwrap_or(0.),
            self.recombination.map_or(0., |r| r.0),
            self.tunnel.unwrap_or(0.),
            self.tunnel_sidewall.unwrap_or(0.),
            self.cjo,
            self.vj,
            self.grading,
            self.cjsw,
            self.vjsw,
            self.tt,
            self.conductance,
        ]
        .iter()
        .all(|v| v.is_finite());
        // VJ/M enter the bottom charge and the recombination generation
        // factor; VJSW only the sidewall charge.
        let bottom = self.cjo != 0. || self.recombination.is_some();
        if !finite
            || self.cjo < 0.
            || self.cjsw < 0.
            || (bottom && (self.vj <= 0. || !(0. ..1.).contains(&self.grading)))
            || (self.cjsw != 0. && self.vjsw <= 0.)
        {
            return Err(SpiceError::Numerical {
                context: "diode".into(),
                message: "temperature-adjusted parameters out of domain \
                          (VJ, VJSW > 0, CJO, CJSW >= 0, 0 <= M < 1)"
                    .into(),
            });
        }
        Ok(())
    }

    /// `dioload.c` currents and charges at junction voltage `vd` (no RSW, no
    /// soft recovery, no self-heating).
    fn point(&self, vd: Real, gmin: Real) -> SpiceResult<DiodePoint> {
        let point = self.point_unchecked(vd, gmin)?;
        point.junction.validate()?;
        if !point.ac_conductance.is_finite() || !point.ac_capacitance.is_finite() {
            return Err(SpiceError::Numerical {
                context: "junction".into(),
                message: "nonfinite small-signal junction values".into(),
            });
        }
        Ok(point)
    }

    /// [`Self::point`] without the final finiteness checks, for a `.sens`
    /// stand-in that must carry C's NaN.
    fn point_unchecked(&self, vd: Real, gmin: Real) -> SpiceResult<DiodePoint> {
        let vte = self.n * self.vt;
        let vtebrk = self.nbv * self.vt;
        let breakdown = self.breakdown.map(|bv| (bv, vtebrk));
        let forward = vd >= -3. * vte;

        let (mut cdsw, mut gdsw) = (0., 0.);
        if let Some(csatsw) = self.csatsw {
            (cdsw, gdsw) = match self.ns {
                Some(ns) => characteristic(vd, ns * self.vt, csatsw, breakdown)?,
                // Common characteristic. The breakdown branch is rejected at
                // instantiation for JSW*PJ > 0, so csatsw is zero there.
                None if forward || breakdown.is_none_or(|(bv, _)| vd >= -bv) => {
                    characteristic(vd, vte, csatsw, None)?
                }
                None => (0., 0.),
            };
        }
        let (mut cdb, mut gdb) = characteristic(vd, vte, self.csat, breakdown)?;
        // C's small-signal bottom conductance (differs only with ISR).
        let mut ac_gdb = gdb;
        if let Some((crec, nr)) = self.recombination {
            let vterec = nr * self.vt;
            let generation = |v: Real| {
                let t1 = (1. - v / self.vj).powi(2) + 0.005;
                (t1, t1.powf(self.grading / 2.))
            };
            if forward {
                let evd = (vd / vterec).exp();
                let current = crec * (evd - 1.);
                let conductance = crec * evd / vterec;
                let (t1, factor) = generation(vd);
                let derivative =
                    -self.grading * (1. - vd / self.vj) * t1.powf(self.grading / 2. - 1.);
                cdb += current * factor;
                // d/dv of the generation factor carries 1/VJ.
                gdb += conductance * factor + current * derivative / self.vj;
                // dioload.c instead multiplies the already-scaled current and
                // omits 1/VJ; its root is unaffected, its AC conductance is.
                ac_gdb += conductance * factor + current * factor * derivative;
            } else {
                // Reverse and breakdown: the constant value at -3*N*Vt.
                let (_, factor) = generation(-3. * vte);
                cdb += factor * crec * ((-3. * vte / vterec).exp() - 1.);
            }
        }
        let vtetun = self.ntun * self.vt;
        if let Some(tunnel) = self.tunnel_sidewall {
            let evd = (-vd / vtetun).exp();
            cdsw -= tunnel * (evd - 1.);
            gdsw += tunnel * evd / vtetun;
        }
        if let Some(tunnel) = self.tunnel {
            let evd = (-vd / vtetun).exp();
            cdb -= tunnel * (evd - 1.);
            let g = tunnel * evd / vtetun;
            gdb += g;
            ac_gdb += g;
        }
        let knee = if forward {
            self.ikf.filter(|_| cdb > 1e-18)
        } else {
            self.ikr.filter(|_| cdb < -1e-18)
        };
        if let Some(k) = knee {
            let factor;
            (cdb, factor) = high_injection(cdb, k);
            gdb *= factor;
            ac_gdb *= factor;
        }
        if let Some(k) = self.ikp.filter(|_| cdsw > 1e-18) {
            let factor;
            (cdsw, factor) = high_injection(cdsw, k);
            gdsw *= factor;
        }
        let current = cdb + cdsw + gmin * vd;
        let conductance = gdb + gdsw + gmin;
        let ac_conductance = ac_gdb + gdsw + gmin;
        let (charge, capacitance) = depletion_charge(vd, self.cjo, self.vj, self.grading, self.fc);
        let (charge_sw, capacitance_sw) =
            depletion_charge(vd, self.cjsw, self.vjsw, self.mjsw, self.fcs);
        let point = DiodePoint {
            junction: JunctionPoint {
                current,
                conductance,
                // dioload.c: diffusion charge TT*cd includes the gmin current.
                charge: charge + charge_sw + self.tt * current,
                capacitance: capacitance + capacitance_sw + self.tt * conductance,
            },
            ac_conductance,
            ac_capacitance: capacitance + capacitance_sw + self.tt * ac_conductance,
        };
        Ok(point)
    }
}

/// One exponential characteristic: forward exponential, C's cubic reverse
/// continuation below `-3*vte` and, with `breakdown = (xbv, n_bv*Vt)`, the
/// reverse-breakdown exponential below `-xbv` (`dioload.c`).
fn characteristic(
    v: Real,
    vte: Real,
    sat: Real,
    breakdown: Option<(Real, Real)>,
) -> SpiceResult<(Real, Real)> {
    match breakdown {
        Some((bv, vtebrk)) if v < -3. * vte && v < -bv => {
            let evrev = (-(bv + v) / vtebrk).exp();
            let result = (-sat * evrev, sat * evrev / vtebrk);
            if !result.0.is_finite() || !result.1.is_finite() {
                return Err(SpiceError::Numerical {
                    context: "junction".into(),
                    message: "reverse-breakdown exponential overflow".into(),
                });
            }
            Ok(result)
        }
        _ if sat == 0. => Ok((0., 0.)),
        _ => junction_current(v, vte, sat),
    }
}

/// `dioload.c` high-injection knee: `i/(1+sqrt(i/k))` with `k` signed like
/// `i`. Returns the limited current and the factor applied to its conductance.
fn high_injection(current: Real, knee: Real) -> (Real, Real) {
    let ratio = (current / knee).abs();
    let root = ratio.sqrt();
    (
        current / (1. + root),
        ((1. + root) - ratio / (2. * root)) / (1. + 2. * root + ratio),
    )
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct JunctionPoint {
    pub current: Real,
    pub conductance: Real,
    pub charge: Real,
    pub capacitance: Real,
}
impl JunctionPoint {
    pub(crate) fn validate(self) -> SpiceResult<()> {
        if [
            self.current,
            self.conductance,
            self.charge,
            self.capacitance,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || self.capacitance < 0.
        {
            return Err(SpiceError::Numerical {
                context: "junction".into(),
                message: "nonfinite/out-of-domain junction equations".into(),
            });
        }
        Ok(())
    }
}
/// The C reverse-bias continuation joins the exponential at -3*Vt.
pub(crate) fn junction_current(v: Real, vt: Real, is: Real) -> SpiceResult<(Real, Real)> {
    let result = if v >= -3. * vt {
        let exp = (v / vt).exp();
        (is * (exp - 1.), is * exp / vt)
    } else {
        let arg = (3. * vt / (v * std::f64::consts::E)).powi(3);
        (-is * (1. + arg), is * 3. * arg / v)
    };
    if [is, vt, result.0, result.1].iter().any(|v| !v.is_finite()) || vt <= 0. || is <= 0. {
        return Err(SpiceError::Numerical {
            context: "junction".into(),
            message: "junction exponential/temperature overflow".into(),
        });
    }
    Ok(result)
}
pub(crate) fn depletion_charge(v: Real, c: Real, p: Real, m: Real, fc: Real) -> (Real, Real) {
    let boundary = fc * p;
    let at = |v: Real| {
        let arg = 1. - v / p;
        (c * p * (1. - arg.powf(1. - m)) / (1. - m), c * arg.powf(-m))
    };
    if v < boundary {
        at(v)
    } else {
        let (q0, c0) = at(boundary);
        let dc = c0 * m / (p - boundary);
        let delta = v - boundary;
        (q0 + c0 * delta + 0.5 * dc * delta * delta, c0 + dc * delta)
    }
}
pub(crate) fn stamp_junction(
    context: &mut StampContext<'_>,
    nodes: [NodeId; 2],
    v: Real,
    p: JunctionPoint,
    slot: usize,
) -> SpiceResult<()> {
    let mut conductance = p.conductance;
    let mut equivalent = p.current - p.conductance * v;
    if let Some(coefficients) = context.integration {
        let crate::devices::AnalysisMode::Transient { dt, .. } = context.mode else {
            return Err(SpiceError::circuit("junction companion outside transient"));
        };
        if dt != coefficients.dt() {
            return Err(SpiceError::circuit("junction timestep mismatch"));
        }
        let mut history = vec![p.charge];
        for age in 1..=coefficients.charge_history_len() {
            history.push(
                context
                    .states
                    .accepted(age, slot)
                    .ok_or_else(|| SpiceError::circuit("missing accepted junction charge"))?,
            );
        }
        let previous = if coefficients.needs_previous_derivative() {
            Some(
                context
                    .states
                    .accepted(1, slot + 1)
                    .ok_or_else(|| SpiceError::circuit("missing accepted junction derivative"))?,
            )
        } else {
            None
        };
        let companion = coefficients.integrate(&history, previous, p.capacitance)?;
        conductance += companion.conductance;
        // For nonlinear Q, integrate()'s current is dQ/dt-a0*Q.
        // Linearize Q around actual v: dq/dt - a0*(dQ/dv)*v.
        equivalent += companion.derivative - companion.conductance * v;
        context.states.set(slot + 1, companion.derivative)?;
    } else {
        if context.mode.is_transient() {
            return Err(SpiceError::circuit(
                "junction transient needs companion integration",
            ));
        }
        context.states.set(slot + 1, 0.)?;
    }
    context.states.set(slot, p.charge)?;
    crate::devices::linear::nodal_stamp(context.matrix, context.unknowns, nodes, conductance)?;
    context.stamp_rhs(nodes[0], -equivalent)?;
    context.stamp_rhs(nodes[1], equivalent)
}
impl Device for Diode {
    fn observation_parameter(
        &self,
        key: &str,
        context: &ModelContext,
    ) -> SpiceResult<Option<Real>> {
        let p = &self.parameters;
        Ok(match key {
            "area" => Some(p.area),
            "pj" | "perim" => Some(p.perimeter),
            "m" => Some(p.multiplier),
            "temp" => Some(p.temperature.unwrap_or(context.temperature + p.dtemp)),
            "dtemp" => Some(p.dtemp),
            _ => None,
        })
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn designator(&self) -> char {
        'd'
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
    fn state_count(&self) -> usize {
        // Charge, its derivative and the limited junction voltage.
        3
    }
    fn truncation_slot(&self) -> Option<usize> {
        Some(0)
    }
    /// `dio.c` `DIOpTable`: AREA, PJ (alias PERIM), M, TEMP and DTEMP, which
    /// `diotemp.c` re-derives completely (`dctrcurv.c` `DCTsetInstParam`).
    fn instance_parameter(&self, keyword: &str) -> Option<&'static str> {
        match keyword.to_ascii_lowercase().as_str() {
            "ic" => Some("ic"),
            "w" => Some("w"),
            "l" => Some("l"),
            "area" => Some("area"),
            "pj" | "perim" => Some("pj"),
            "m" => Some("m"),
            "temp" => Some("temp"),
            "dtemp" => Some("dtemp"),
            _ => None,
        }
    }
    /// `DIOparam` then `DIOtemp`: the swept setter replaces the instance value
    /// (AREA/PJ outrank the model's), every temperature-dependent quantity is
    /// re-derived from it, and the instance schema domains still apply. A
    /// swept DTEMP on a diode with an instance TEMP is rejected: C silently
    /// ignores DTEMP there (`diotemp.c`), as the card-level rule says.
    fn with_instance_parameter(
        &self,
        parameter: &str,
        value: Real,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        let mut initial = self.initial.clone();
        if let Some(index) = ["ic"].iter().position(|k| *k == parameter) {
            crate::devices::sweep::check_swept(&self.name, parameter, value, true, "finite")?;
            initial.values[index] = Some(value);
        }
        let mut p = self.parameters;
        let domain = |ok: bool, what: &str| {
            if ok && value.is_finite() {
                Ok(())
            } else {
                Err(SpiceError::circuit(format!(
                    "{}: swept {parameter}={value} must be {what}",
                    self.name
                )))
            }
        };
        match parameter {
            "ic" => {}
            "w" | "l" => {
                domain(value >= 0., "nonnegative")?; /* diotemp.c geometry is setup-only once area was resolved. */
            }
            "area" => {
                domain(value > 0., "positive")?;
                p.area = value;
            }
            "pj" => {
                domain(value >= 0., "nonnegative")?;
                p.perimeter = value;
            }
            "m" => {
                domain(value > 0., "positive")?;
                p.multiplier = value;
            }
            "temp" => {
                domain(value + 273.15 > 0., "above absolute zero")?;
                p.temperature = Some(value);
            }
            "dtemp" if p.temperature.is_some() => {
                return Err(SpiceError::Unsupported {
                    feature: format!(
                        "{}: swept dtemp on a diode with an instance temp (C ignores dtemp)",
                        self.name
                    ),
                    location: Some(self.location.clone()),
                });
            }
            "dtemp" => {
                domain(true, "finite")?;
                p.dtemp = value;
            }
            _ => {
                return Err(SpiceError::circuit(format!(
                    "{}: diode parameter {parameter} cannot be swept",
                    self.name
                )));
            }
        }
        p.check(&self.location)?;
        p.thermal(context)?.point(0., context.gmin)?;
        Ok(Box::new(Self {
            name: self.name.clone(),
            model: self.model.clone(),
            terminals: self.terminals.clone(),
            junction: self.junction,
            parameters: p,
            initial,
            location: self.location.clone(),
            written: self.written.clone(),
            lenient: false,
        }))
    }
    /// `.sens`: C's diode records ([`sens`]).
    fn sensitivity(
        &self,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn crate::devices::sensitivity::DeviceSensitivity + '_>> {
        Ok(Box::new(sens::DiodeSensitivity::new(self, context)))
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit("diode AC needs small-signal assembly"));
        }
        let raw = context.node_voltage(self.junction[0]) - context.node_voltage(self.junction[1]);
        let model = context.model_context();
        let thermal = self.parameters.thermal(&model)?;
        let mut limiter = Limiter::new(&context.states);
        // diogetic.c takes the uic start across the external terminals unless
        // `DIOinitCondGiven`, which no C setter ever sets (`dioparam.c`
        // stores `ic=` without it): the instance `ic=` never reaches the load.
        let start =
            context.node_voltage(self.terminals[0]) - context.node_voltage(self.terminals[1]);
        let v = thermal.limit(&mut limiter, &context.states, raw, start, self.initial.off);
        let mut p = if self.lenient {
            thermal.point_unchecked(v, model.gmin)?
        } else {
            thermal.point(v, model.gmin)?
        };
        // C's DEVload matrix (`.tf`, `.sens`): dioload.c's junction
        // conductance, which differs from the exact derivative only with ISR.
        if context.states.c_jacobian() {
            p.junction.conductance = p.ac_conductance;
        }
        // DIOconvTest for an `off` diode held in MODEINITFIX.
        let held = p.junction.current;
        limiter.test_held(
            &context.states,
            &[(held, held + p.junction.conductance * (raw - v))],
        );
        if self.parameters.rs > 0. {
            crate::devices::linear::nodal_stamp(
                context.matrix,
                context.unknowns,
                [self.terminals[0], self.junction[0]],
                thermal.conductance,
            )?;
        }
        stamp_junction(context, self.junction, v, p.junction, 0)?;
        limiter.finish(&mut context.states, &[(DIODE_VOLTAGE_SLOT, v)])
    }
    /// `dioacld.c`: series conductance, C's small-signal junction conductance
    /// and `j*omega` times its small-signal capacitance at the bias point.
    fn assemble_small_signal(
        &self,
        context: &mut LinearContext<'_>,
        bias: &Vector,
    ) -> SpiceResult<()> {
        let v = |node| {
            context
                .unknowns
                .node_row(node)
                .and_then(|r| bias.get(r))
                .unwrap_or(0.)
        };
        let thermal = self.parameters.thermal(context.model_context)?;
        let p = thermal.point(
            v(self.junction[0]) - v(self.junction[1]),
            context.model_context.gmin,
        )?;
        if self.parameters.rs > 0. {
            context.nodal(
                [self.terminals[0], self.junction[0]],
                thermal.conductance,
                false,
            )?;
        }
        context.nodal(self.junction, p.ac_conductance, false)?;
        context.nodal(self.junction, p.ac_capacitance, true)
    }

    /// Pole-zero load: C `diopzld.c` equals the AC load with `s` for `j omega`.
    fn assemble_pole_zero(
        &self,
        context: &mut crate::devices::linear::LinearContext<'_>,
        bias: &crate::maths::Vector,
    ) -> crate::primitives::SpiceResult<()> {
        self.assemble_small_signal(context, bias)
    }

    /// `diodset.c`/`diodisto.c`: the junction current's and charge's second-
    /// and third-order Taylor coefficients in the junction voltage, from C's
    /// own simplified distortion model rather than `dioload.c`: the ideal
    /// exponential of the total (bottom plus sidewall) saturation current,
    /// SPICE3's cubic reverse law below `-3 N Vt`, a breakdown exponential in
    /// `Vt` (not `NBV Vt`) below `-BV`, no ISR/IKF/IKR/tunnelling/gmin terms,
    /// the transit-time diffusion charge, and the depletion charges graded
    /// against the model's **unadjusted** VJ/VJSW below the temperature-
    /// adjusted `FC*VJ` (`DIOtDepCap`, which C applies to the sidewall too).
    /// The charge term is omitted when its second-order coefficient is zero,
    /// as `diodisto.c` skips it.
    fn distortion(
        &self,
        context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        use crate::devices::distortion::{
            Control, DeviceDistortion, DistortionTerm, Response, Taylor,
        };
        let p = &self.parameters;
        let t = p.thermal(context.model_context)?;
        let vd = context.voltage(self.junction[0]) - context.voltage(self.junction[1]);
        let csat = t.csat + t.csatsw.unwrap_or(0.);
        let vt = t.vt;
        let vte = t.n * vt;
        let tt = t.tt;
        let breakdown = t.breakdown.filter(|bv| *bv != 0.);
        let (g2, g3, cdiff2, cdiff3) = if vd >= -3. * vte {
            let evd = (vd / vte).exp();
            let gd = csat * evd / vte;
            let g2 = 0.5 * gd / vte;
            let g3 = g2 / 3. / vte;
            (g2, g3, g2 * tt, g3 * tt)
        } else if breakdown.is_none_or(|bv| vd >= -bv) {
            let arg = 3. * vte / (vd * std::f64::consts::E);
            let arg = arg * arg * arg;
            let gd = csat * 3. * arg / vd;
            let g2 = -4. * gd / vd;
            (g2, 5. * g2 / vd, 0., 0.)
        } else {
            let bv = breakdown.unwrap_or(0.);
            let evrev = (-(bv + vd) / vt).exp();
            let gd = csat * evrev / vt;
            let g2 = -gd / 2. / vt;
            (g2, -g2 / 3. / vt, 0., 0.)
        };
        let depletion_cap = p.fc * t.vj;
        // `diotemp.c`: DIOtF2 = exp((1 + M(T)) ln(1 - FC)).
        let junction = |czero: Real, pot: Real, grading: Real, f2: Real| {
            if czero == 0. {
                (0., 0.)
            } else if vd < depletion_cap {
                let arg = 1. - vd / pot;
                let sarg = (-grading * arg.ln()).exp();
                let c1 = czero * sarg;
                let c2 = c1 / 2. / pot * grading / arg;
                let c3 = c2 / 3. / pot / arg * (grading + 1.);
                (c2, c3)
            } else {
                (czero / f2 / 2. / pot * grading, 0.)
            }
        };
        let f2 = ((1. + t.grading) * (1. - p.fc).ln()).exp();
        let f2_sw = ((1. + p.mjsw) * (1. - p.fcs).ln()).exp();
        let (cjunc2, cjunc3) = junction(t.cjo, p.vj, t.grading, f2);
        let (sw2, sw3) = junction(t.cjsw, p.vjsw, p.mjsw, f2_sw);
        let (cap2, cap3) = (cdiff2 + (cjunc2 + sw2), cdiff3 + (cjunc3 + sw3));
        let control = || vec![Control::between(self.junction[0], self.junction[1])];
        let mut terms = vec![DistortionTerm::new(
            Response::Current,
            self.junction,
            control(),
            Taylor::single(g2, g3),
        )];
        if cap2 != 0. {
            terms.push(DistortionTerm::new(
                Response::Charge,
                self.junction,
                control(),
                Taylor::single(cap2, cap3),
            ));
        }
        Ok(DeviceDistortion::Terms(terms))
    }

    /// `dionoise.c`: thermal noise of the series resistance at the instance
    /// temperature, shot noise `2 q abs(cd)` of the junction current (gmin
    /// current included, as C's `DIOcurrent`) and the flicker law
    /// `KF abs(cd/m)^AF m / f`. The `_rsw`/`_idsw`/`_1overfsw` generators of
    /// a separate sidewall (RSW, not ported) are zero, as C's are without RSW.
    fn noise(&self, context: &NoiseContext<'_>) -> SpiceResult<DeviceNoise> {
        let p = &self.parameters;
        let thermal = p.thermal(context.model_context)?;
        let vd = context.voltage(self.junction[0]) - context.voltage(self.junction[1]);
        let cd = thermal
            .point(vd, context.model_context.gmin)?
            .junction
            .current;
        let temperature = p
            .temperature
            .unwrap_or(context.model_context.temperature + p.dtemp)
            + 273.15;
        let m = p.multiplier;
        let flicker = p.kf * (p.af * (cd / m).abs().max(1e-38).ln()).exp() * m;
        let [anode, cathode] = self.junction;
        let external = self.terminals[0];
        Ok(DeviceNoise::Sources {
            family: NoiseFamily::Diode,
            model: Some(self.model.clone()),
            total: true,
            sources: vec![
                NoiseSource::new(
                    "_rs",
                    [anode, external],
                    NoiseKind::Thermal {
                        conductance: thermal.conductance,
                        temperature,
                    },
                ),
                NoiseSource::new("_id", self.junction, NoiseKind::Shot { current: cd }),
                NoiseSource::new(
                    "_1overf",
                    self.junction,
                    NoiseKind::Flicker {
                        coefficient: flicker,
                        exponent: 1.,
                    },
                ),
                NoiseSource::new(
                    "_rsw",
                    [anode, external],
                    NoiseKind::Thermal {
                        conductance: 0.,
                        temperature,
                    },
                ),
                NoiseSource::new("_idsw", [anode, cathode], NoiseKind::Shot { current: 0. }),
                NoiseSource::new(
                    "_1overfsw",
                    [anode, cathode],
                    NoiseKind::Flicker {
                        coefficient: 0.,
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
    use crate::devices::models::ModelResolver;
    use crate::netlist::{Parser, source::parse_deck_text};
    use std::path::Path;

    fn deck(model: &str, instance: &str) -> crate::netlist::ast::Netlist {
        Parser::new()
            .parse_deck(&parse_deck_text(
                Path::new("diode.cir"),
                &format!("diode\nd1 a 0 dm {instance}\n.model dm d({model})\n.end\n"),
            ))
            .unwrap()
    }
    fn parameters(model: &str, instance: &str) -> SpiceResult<DiodeParameters> {
        let n = deck(model, instance);
        let resolver = ModelResolver::new(&n.models)?;
        let instance = &n.devices[0];
        let resolved = resolver.resolve(instance)?.unwrap();
        let setters = canonical(
            resolved
                .card()
                .parameters
                .iter()
                .filter(|p| !p.name.eq_ignore_ascii_case("level")),
            MODEL_ALIASES,
            MODEL_PENDING,
            "model",
        )?;
        let m = MODEL.validate(&setters, &resolved.card().location)?;
        let (_, setters) = crate::devices::initial::split(&instance.parameters, &["ic"])?;
        let setters = canonical(&setters, &[], INSTANCE_PENDING, "instance")?;
        let i = INSTANCE.validate(&setters, &instance.location)?;
        DiodeParameters::new(&m, &i, instance)
    }
    fn thermal(model: &str, instance: &str, celsius: Real) -> Thermal {
        parameters(model, instance)
            .unwrap()
            .thermal(&ModelContext::new(celsius, 27.))
            .unwrap()
    }
    fn instantiate(model: &str, instance: &str) -> SpiceResult<Box<dyn Device>> {
        let n = deck(model, instance);
        let resolver = ModelResolver::new(&n.models)?;
        let resolved = resolver.resolve(&n.devices[0])?.unwrap();
        Diode::instantiate(
            &n.devices[0],
            &mut NodeTable::new(),
            &resolved,
            &ModelContext::default(),
        )
    }

    /// Central differences of current and charge against the analytic
    /// conductance and capacitance, away from C's region boundaries.
    fn assert_derivatives(t: &Thermal, voltages: &[Real]) {
        for &v in voltages {
            let h = 1e-6 * v.abs().max(1e-2);
            let p = t.point(v, 1e-12).unwrap().junction;
            let (plus, minus) = (
                t.point(v + h, 1e-12).unwrap().junction,
                t.point(v - h, 1e-12).unwrap().junction,
            );
            let g = (plus.current - minus.current) / (2. * h);
            let c = (plus.charge - minus.charge) / (2. * h);
            let current_scale = p.current.abs().max(plus.current.abs()) / h;
            assert!(
                (g - p.conductance).abs() <= 1e-5 * p.conductance.abs() + 1e-9 * current_scale,
                "{v}: dI/dV {g} vs {}",
                p.conductance
            );
            let charge_scale = p.charge.abs().max(plus.charge.abs()) / h;
            assert!(
                (c - p.capacitance).abs() <= 1e-5 * p.capacitance.abs() + 1e-9 * charge_scale,
                "{v}: dQ/dV {c} vs {}",
                p.capacitance
            );
        }
    }

    #[test]
    fn charge_derivative_and_current_jacobian_are_consistent() {
        for v in [-1., -0.08, -0.02, 0., 0.49, 0.5, 0.51, 0.7] {
            let h = 1e-7;
            let (q, c) = depletion_charge(v, 2e-12, 1., 0.5, 0.5);
            let derivative = (depletion_charge(v + h, 2e-12, 1., 0.5, 0.5).0
                - depletion_charge(v - h, 2e-12, 1., 0.5, 0.5).0)
                / (2. * h);
            assert!(
                (derivative - c).abs() < 1e-8 * c.abs(),
                "{v}: {q} {derivative} {c}"
            );
            let (i, g) = junction_current(v, 0.026, 1e-14).unwrap();
            let derivative = (junction_current(v + h, 0.026, 1e-14).unwrap().0
                - junction_current(v - h, 0.026, 1e-14).unwrap().0)
                / (2. * h);
            assert!(
                (derivative - g).abs() < 1e-7 * g.abs() + 4. * f64::EPSILON * 1e-14 / h,
                "{v}: {i} {derivative} {g}"
            );
        }
    }

    const SWEEP: &[Real] = &[
        -9., -6.2, -5.4, -5.0, -3., -0.5, -0.05, 0.1, 0.4, 0.62, 0.75,
    ];

    #[test]
    fn breakdown_jacobian_charge_and_matching() {
        for celsius in [-40., 27., 125.] {
            let t = thermal(
                "is=1e-14 n=1.1 bv=5.1 ibv=5m nbv=1.4 tcv=2m cjo=30p vj=0.7 m=0.4 tt=5n",
                "",
                celsius,
            );
            assert_derivatives(&t, SWEEP);
        }
        // At nominal temperature the knee carries IBV at V = -BV (diotemp.c).
        let t = thermal("is=1e-14 bv=5.1 ibv=5m nbv=1.4", "m=2", 27.);
        let i = t.point(-5.1, 0.).unwrap().junction.current;
        assert!((i + 2. * 5e-3).abs() < 1e-8 * 1e-2, "{i}");
        // TCV shifts the breakdown voltage linearly (TLEV 0) or relatively
        // (TLEV 1) before matching against the heated saturation current.
        for (tlev, tbv) in [(0, 10. - 1e-3 * 100.), (1, 10. * (1. - 1e-3 * 100.))] {
            let t = thermal(&format!("bv=10 tcv=1m tlev={tlev} ibv=1e-3"), "", 127.);
            let expected = breakdown_voltage(tbv, 1e-3, t.csat, t.vt, 1.).unwrap();
            assert_eq!(t.breakdown, Some(expected));
            assert!(expected < tbv && expected > tbv - 0.5);
        }
        // IBV below IS*BV/Vt keeps the unmatched BV, like C.
        assert_eq!(
            thermal("is=1e-6 bv=50 ibv=1e-6", "", 27.).breakdown,
            Some(50.)
        );
    }

    #[test]
    fn sidewall_jacobian_and_charge_for_both_characteristics() {
        for (model, instance) in [
            (
                "is=1e-14 jsw=2e-15 ns=1.6 cjsw=4p vjsw=0.8 mjsw=0.3 fcs=0.4 bv=5.1",
                "pj=10",
            ),
            (
                "is=1e-14 jsw=2e-15 cjsw=4p vjsw=0.8 mjsw=0.3 fcs=0.4 ikp=1u",
                "pj=10 m=2",
            ),
            ("cjsw=4p php=0.6 cjo=10p", "perim=3"),
        ] {
            for celsius in [-20., 27., 90.] {
                assert_derivatives(&thermal(model, instance, celsius), SWEEP);
            }
        }
        // PJ scales the sidewall saturation current and capacitance.
        let one = thermal("jsw=1e-15 cjsw=1p", "pj=1", 27.)
            .point(0.5, 0.)
            .unwrap();
        let ten = thermal("jsw=1e-15 cjsw=1p", "pj=10", 27.)
            .point(0.5, 0.)
            .unwrap();
        let bottom = thermal("", "", 27.).point(0.5, 0.).unwrap();
        let sidewall = |p: DiodePoint| p.junction.current - bottom.junction.current;
        assert!((sidewall(ten) / sidewall(one) - 10.).abs() < 1e-9);
        assert!((ten.junction.capacitance / one.junction.capacitance - 10.).abs() < 1e-9);
        // The model PJ is the instance default.
        let model_pj = thermal("jsw=1e-15 pj=10", "", 27.).point(0.5, 0.).unwrap();
        assert!((sidewall(model_pj) / sidewall(one) - 10.).abs() < 1e-9);
    }

    #[test]
    fn recombination_tunnelling_and_knee_jacobians() {
        for model in [
            "isr=1e-11 nr=2.2 vj=0.8 m=0.4 cjo=5p",
            "jtun=1e-13 jtunsw=1e-14 ntun=20 xtitun=2 keg=0.9 bv=7",
            "ikf=1m ikr=1n is=1e-12",
            "isr=1e-11 jtun=1e-12 ikf=1m ikr=1u bv=5.1 jsw=1e-15 ns=1.2",
        ] {
            for celsius in [-40., 27., 100.] {
                assert_derivatives(&thermal(model, "pj=4", celsius), SWEEP);
            }
        }
        // C's small-signal conductance equals the Newton derivative except for
        // the recombination term, whose dioload.c conductance omits 1/VJ and
        // reapplies the generation factor.
        let plain = thermal("ikf=1m jtun=1e-12 tt=1n", "", 27.)
            .point(0.6, 1e-12)
            .unwrap();
        assert_eq!(plain.ac_conductance, plain.junction.conductance);
        assert_eq!(plain.ac_capacitance, plain.junction.capacitance);
        let t = thermal("isr=1e-11 vj=0.8 m=0.4", "", 27.);
        let p = t.point(0.6, 0.).unwrap();
        let (vterec, vj, m, v) = (2. * t.vt, 0.8, 0.4, 0.6);
        let current = 1e-11 * ((v / vterec).exp() - 1.);
        let t1: Real = (1. - v / vj).powi(2) + 0.005;
        let factor = t1.powf(m / 2.);
        let c_derivative = -m * (1. - v / vj) * t1.powf(m / 2. - 1.);
        let bottom = t.csat * (v / t.vt).exp() / t.vt;
        let expected =
            bottom + 1e-11 * (v / vterec).exp() / vterec * factor + current * factor * c_derivative;
        assert!((p.ac_conductance - expected).abs() < 1e-12 * expected);
        assert!(p.ac_conductance != p.junction.conductance);
    }

    #[test]
    fn temperature_laws_follow_diotemp() {
        // Nominal temperature leaves VJ, CJO and IS unchanged.
        let t = thermal("cjo=10p vj=0.7 m=0.4 cjsw=1p vjsw=0.6", "pj=2", 27.);
        assert!((t.vj - 0.7).abs() < 1e-14 && (t.cjo - 10e-12).abs() < 1e-24);
        assert!((t.vjsw - 0.6).abs() < 1e-14 && (t.cjsw - 2e-12).abs() < 1e-24);
        assert!((t.csat - 1e-14).abs() < 1e-28);
        // TLEV 0: IS(T) = IS exp((T/Tn - 1) EG/(N Vt) + XTI/N ln(T/Tn)).
        let (tk, tn) = (398.15, 300.15);
        let vt = K_OVER_Q * tk;
        let t = thermal("is=1e-14 n=1.2 eg=1.2 xti=2", "", 125.);
        let expected =
            1e-14 * ((tk / tn - 1.) * 1.2 / (1.2 * vt) + 2. / 1.2 * (tk / tn).ln()).exp();
        assert!((t.csat / expected - 1.).abs() < 1e-12);
        // DTEMP offsets the circuit temperature; TEMP replaces it.
        let offset = thermal("is=1e-14", "dtemp=98", 27.);
        assert!((offset.csat / thermal("is=1e-14", "", 125.).csat - 1.).abs() < 1e-12);
        let fixed = thermal("is=1e-14", "temp=125", -40.);
        assert!((fixed.csat / offset.csat - 1.).abs() < 1e-12);
        // TNOM moves the reference temperature.
        let tnom = thermal("is=1e-14 tnom=125", "", 125.);
        assert!((tnom.csat - 1e-14).abs() < 1e-28);
        // TLEVC 1: linear CTA/TPB laws about REFTEMP.
        let t = thermal(
            "cjo=10p vj=0.7 tlevc=1 cta=1m tpb=2m cjsw=1p vjsw=0.5 ctp=3m tphp=1m",
            "pj=1",
            77.,
        );
        assert!((t.cjo - 10e-12 * 1.05).abs() < 1e-24 && (t.vj - 0.6).abs() < 1e-12);
        assert!((t.cjsw - 1e-12 * 1.15).abs() < 1e-24 && (t.vjsw - 0.45).abs() < 1e-12);
        // TLEVC 0 lowers the junction potential and raises CJO when heated.
        let hot = thermal("cjo=10p vj=0.7 m=0.4", "", 125.);
        assert!(hot.vj < 0.7 && hot.cjo > 10e-12);
        // TLEV 2 uses the EG/GAP1/GAP2 band gap; TT/RS coefficients apply.
        let t = thermal(
            "tlev=2 eg=1.16 gap1=7.02e-4 gap2=1108 tt=1n ttt1=1m rs=10 trs1=2m",
            "",
            127.,
        );
        assert!((t.tt - 1.1e-9).abs() < 1e-21);
        assert!((t.conductance - 1. / (10. * 1.2)).abs() < 1e-12);
        for celsius in [-55., 27., 150.] {
            assert_derivatives(
                &thermal(
                    "tlev=2 tlevc=0 cjo=20p vj=0.75 m=0.45 tm1=1m isr=1e-12 bv=12",
                    "",
                    celsius,
                ),
                SWEEP,
            );
        }
    }

    #[test]
    fn aliases_keep_last_set_precedence() {
        let p = parameters("cj0=1p cjo=2p cj=3p pb=0.6 mj=0.3 vb=7 ib=1u nz=2", "").unwrap();
        assert_eq!((p.cjo, p.vj, p.grading), (3e-12, 0.6, 0.3));
        assert_eq!((p.bv, p.ibv, p.nbv), (Some(7.), 1e-6, 2.));
        let p = parameters("cjo=2p cj0=1p", "").unwrap();
        assert_eq!(p.cjo, 1e-12);
        // NBV defaults to N; EG to 1.11 eV, or 1.16 eV under TLEV 2.
        let p = parameters("n=1.3 tlev=2", "").unwrap();
        assert_eq!((p.nbv, p.eg), (1.3, 1.16));
        assert_eq!(parameters("", "").unwrap().eg, 1.11);
    }

    #[test]
    fn unported_and_invalid_setters_fail_explicitly() {
        for (model, instance) in [
            ("rsw=1", ""),
            ("vp=1 tt=1n", ""),
            ("rth0=10", ""),
            ("bv_max=10", ""),
            ("xom=1e4", ""),
            ("", "w=1u l=1u"),
            ("jsw=1e-15 bv=5", "pj=1"),
            ("cjsw=1p tm1=1m", "pj=1"),
        ] {
            let error = instantiate(model, instance).unwrap_err();
            assert!(error.is_not_yet_ported(), "{model} {instance}: {error}");
        }
        // JSW with NS (own characteristic) supports breakdown; JSW*PJ = 0 is
        // harmless under a shared characteristic.
        assert!(instantiate("jsw=1e-15 ns=1 bv=5", "pj=1").is_ok());
        assert!(instantiate("jsw=1e-15 bv=5", "").is_ok());
        // OFF and IC (#99) are ported.
        assert!(instantiate("", "off ic=0.5").is_ok());
        for (model, instance) in [
            ("tlev=3", ""),
            ("tlevc=2", ""),
            ("mjsw=1", ""),
            ("fcs=1", ""),
            ("bv=0", ""),
            ("", "temp=30 dtemp=2"),
            ("unknown=1", ""),
        ] {
            let error = instantiate(model, instance).unwrap_err();
            assert!(!error.is_not_yet_ported(), "{model} {instance}: {error}");
        }
    }
}
