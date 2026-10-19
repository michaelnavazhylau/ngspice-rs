//! Instance setup: `b3set.c`'s instance defaulting, `b3temp.c`'s
//! size-dependent and temperature-dependent parameters, and `b3check.c`'s
//! model/instance diagnostics. This is geometry and parameter setup only: no
//! nodes are interned, no states are allocated and no load equations run.
use std::collections::BTreeMap;

use super::defaults::ModelDefaults;
use super::temperature::{METER2MICRON, TemperatureModel};
use super::{Diagnostic, Inputs, Value};
use crate::primitives::{Real, SourceLoc, SpiceError, SpiceResult};

const EXP_THRESHOLD: Real = 34.0;
const MIN_EXP: Real = 1.713_908_431e-15;

/// A C printf `%g` spelling for `b3check.c` messages.
fn c_g(value: Real) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0".into()
        } else {
            "0".into()
        };
    }
    if !value.is_finite() {
        return format!("{value}");
    }
    let scientific = format!("{value:.5e}");
    let (mantissa, exponent) = scientific.split_once('e').expect("scientific form");
    let exponent: i32 = exponent.parse().expect("exponent");
    if !(-4..6).contains(&exponent) {
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        format!(
            "{mantissa}e{}{:02}",
            if exponent < 0 { '-' } else { '+' },
            exponent.unsigned_abs()
        )
    } else {
        let decimals = usize::try_from(5 - exponent).unwrap_or(0);
        let fixed = format!("{value:.decimals$}");
        let fixed = fixed
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string();
        if fixed.is_empty() || fixed == "-" {
            "0".into()
        } else {
            fixed
        }
    }
}

/// Effective geometry, size-dependent parameters and instance-level setup
/// results. Every `pparam` entry is a `b3temp.c` `pParam` field by keyword.
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceSetup {
    /// Scaled deck L (C `here->BSIM3l`), after the 5 µm default.
    pub length: Real,
    /// Scaled deck W (C `here->BSIM3w`), after the 5 µm default.
    pub width: Real,
    /// Effective length (C `pParam->BSIM3leff`).
    pub leff: Real,
    /// Effective width (C `pParam->BSIM3weff`).
    pub weff: Real,
    /// Effective length for C-V.
    pub leff_cv: Real,
    /// Effective width for C-V.
    pub weff_cv: Real,
    /// Model polarity (+1 NMOS, -1 PMOS).
    pub polarity: i8,
    /// Instance multiplicity (C `here->BSIM3m`).
    pub multiplier: Real,
    /// NQS selector after instance/model resolution.
    pub nqs_mod: i32,
    /// AC NQS selector after instance/model resolution.
    pub acnqs_mod: i32,
    /// Geometry selector (C `here->BSIM3geo`).
    pub geo: i32,
    /// Drain/source junction areas (m²).
    pub drain_area: Real,
    /// Source junction area (m²).
    pub source_area: Real,
    /// Drain junction perimeter (m).
    pub drain_perimeter: Real,
    /// Source junction perimeter (m).
    pub source_perimeter: Real,
    /// Drain squares (ACM off: C defaults to 1).
    pub drain_squares: Real,
    /// Source squares (ACM off: C defaults to 1).
    pub source_squares: Real,
    /// Instance threshold shift (V).
    pub delvto: Real,
    /// Instance mobility multiplier.
    pub mulu0: Real,
    /// Initial VBS.
    pub ic_vbs: Real,
    /// Initial VDS.
    pub ic_vds: Real,
    /// Initial VGS.
    pub ic_vgs: Real,
    /// Instance threshold after `delvto` (C `here->BSIM3vth0`).
    pub vth0: Real,
    /// Instance flat-band voltage after `delvto`.
    pub vfb: Real,
    /// Instance zero-bias flat-band voltage after `delvto`.
    pub vfbzb: Real,
    /// Temperature-adjusted mobility times `mulu0`.
    pub u0temp: Real,
    /// C's recomputed `tconst` (see `InstanceSetup::new` for its quirk).
    pub tconst: Real,
    /// Gate-source overlap capacitance (F).
    pub cgso: Real,
    /// Gate-drain overlap capacitance (F).
    pub cgdo: Real,
    /// Drain series conductance (S).
    pub drain_conductance: Real,
    /// Source series conductance (S).
    pub source_conductance: Real,
    /// Source junction saturation current (A).
    pub source_sat_current: Real,
    /// Drain junction saturation current (A).
    pub drain_sat_current: Real,
    /// Source junction breakdown voltage (0 when C leaves it unset).
    pub vjsm: Real,
    /// Drain junction breakdown voltage (0 when C leaves it unset).
    pub vjdm: Real,
    /// Source junction breakdown current (0 when C leaves it unset).
    pub isevjsm: Real,
    /// Drain junction breakdown current (0 when C leaves it unset).
    pub isevjdm: Real,
    /// All size-dependent and derived `b3temp.c` parameters by keyword.
    pparam: BTreeMap<&'static str, Real>,
    /// Nonfatal `b3temp.c`/`b3check.c` messages in C order.
    pub warnings: Vec<Diagnostic>,
}

/// Mutable model state that C keeps in one `BSIM3model` struct and that
/// `b3check.c` can reset while setting up an instance. Reuse one value for
/// every instance of a model: the resets persist, as they do in C.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelSetup {
    defaults: ModelDefaults,
    temperature: TemperatureModel,
    cgdo: Real,
    cgso: Real,
    cgbo: Real,
    calcacm: i32,
}

impl ModelSetup {
    /// Capture the defaulted model and its temperature pass.
    ///
    /// # Errors
    /// Missing internal model fields.
    pub fn new(defaults: ModelDefaults, temperature: TemperatureModel) -> SpiceResult<Self> {
        Ok(Self {
            cgdo: model_value(&defaults, "cgdo")?,
            cgso: model_value(&defaults, "cgso")?,
            cgbo: model_value(&defaults, "cgbo")?,
            calcacm: model_int(&defaults, "calcacm")?,
            defaults,
            temperature,
        })
    }

    /// The model fields this state was built from.
    #[must_use]
    pub const fn defaults(&self) -> &ModelDefaults {
        &self.defaults
    }

    /// The model-level temperature pass.
    #[must_use]
    pub const fn temperature(&self) -> &TemperatureModel {
        &self.temperature
    }

    /// Set up one instance against the shared model state, applying any
    /// `b3check.c` model resets in C order.
    ///
    /// # Errors
    /// See [`InstanceSetup::new`].
    pub fn setup_instance(
        &mut self,
        instance: &Inputs,
        location: &SourceLoc,
    ) -> SpiceResult<InstanceSetup> {
        InstanceSetup::with_model(self, instance, location)
    }
}

/// Every `b3temp.c` size-dependent parameter keyword, in C order.
#[must_use]
pub fn size_dependent_names() -> Vec<&'static str> {
    super::size_data::SIZE_DEPENDENT
        .iter()
        .map(|row| row.0)
        .collect()
}

impl InstanceSetup {
    /// A size-dependent/derived parameter by keyword, e.g. `phi` or `rdsw`.
    #[must_use]
    pub fn pparam(&self, name: &str) -> Option<Real> {
        self.pparam.get(name).copied()
    }

    /// Apply `b3set.c` instance defaults, `b3temp.c` size/temperature
    /// processing and `b3check.c` parameter checks for one instance.
    ///
    /// C's `tconst` recomputation uses the `sqrt(1 + nlx/leff)` temporary that
    /// survives from the `vfbzb` block rather than `leffCV²`; that observable
    /// quirk is reproduced deliberately. Model mutations made by
    /// `b3check.c` (CGDO/CGSO/CGBO/A2/RDSW/CALCACM resets) apply to this
    /// instance only: the multi-instance shared-model ordering belongs to the
    /// later model/instance layer.
    ///
    /// # Errors
    /// ACM geometry (unported `ACM_SourceDrainResistances`), fatal
    /// `b3check.c` diagnostics, effective geometry <= 0, and nonfinite
    /// derived parameters.
    pub fn new(
        model: &ModelDefaults,
        temperature: &TemperatureModel,
        instance: &Inputs,
        location: &SourceLoc,
    ) -> SpiceResult<Self> {
        let mut shared = ModelSetup::new(model.clone(), temperature.clone())?;
        Self::with_model(&mut shared, instance, location)
    }

    fn with_model(
        shared: &mut ModelSetup,
        instance: &Inputs,
        location: &SourceLoc,
    ) -> SpiceResult<Self> {
        let ModelSetup {
            defaults: model,
            temperature,
            cgdo: model_cgdo,
            cgso: model_cgso,
            cgbo: model_cgbo,
            calcacm,
        } = shared;
        let acm = model_int(model, "acm")?;
        if acm != 0 {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "{location}: model BSIM3 ACM={acm}: the ACM source/drain geometry model is not ported"
                ),
                "src/spicelib/devices/bsim3/b3set.c (ACM_SourceDrainResistances), b3temp.c",
            ));
        }
        let mut warnings = Vec::new();
        let calcacm_value = *calcacm;
        if calcacm_value > 0 && acm != 12 {
            warnings.push(Diagnostic::warning(
                format!("Warning: CALCACM = {calcacm_value} is wrong. Set back to 0."),
                "src/spicelib/devices/bsim3/b3check.c",
            ));
            // C leaves the model field at zero, so later instances stay quiet.
            *calcacm = 0;
        }

        // `b3set.c` instance defaults, in C order.
        let given = |name: &str| instance.get(name).is_some();
        let real = |name: &str| instance_real(instance, name);
        let length = if given("l") { real("l")? } else { 5.0e-6 };
        let width = if given("w") { real("w")? } else { 5.0e-6 };
        let drain_area = if given("ad") { real("ad")? } else { 0.0 };
        let source_area = if given("as") { real("as")? } else { 0.0 };
        let drain_perimeter = if given("pd") { real("pd")? } else { 0.0 };
        let source_perimeter = if given("ps") { real("ps")? } else { 0.0 };
        let default_squares = if acm == 0 { 1.0 } else { 0.0 };
        let drain_squares = if given("nrd") {
            real("nrd")?
        } else {
            default_squares
        };
        let source_squares = if given("nrs") {
            real("nrs")?
        } else {
            default_squares
        };
        let delvto = if given("delvto") {
            real("delvto")?
        } else {
            0.0
        };
        let mulu0 = if given("mulu0") { real("mulu0")? } else { 1.0 };
        let ic_vbs = if given("icvbs") { real("icvbs")? } else { 0.0 };
        let ic_vds = if given("icvds") { real("icvds")? } else { 0.0 };
        let ic_vgs = if given("icvgs") { real("icvgs")? } else { 0.0 };
        let multiplier = if given("m") { real("m")? } else { 1.0 };
        if !(multiplier.is_finite() && multiplier > 0.0) {
            return Err(SpiceError::parse(
                location.clone(),
                "BSIM3 instance multiplier must be finite and positive",
            ));
        }
        let geo = if given("geo") {
            integer(instance, "geo")?
        } else {
            0
        };
        let model_nqs = model_int(model, "nqsmod")?;
        let model_acnqs = model_int(model, "acnqsmod")?;
        let resolve_selector = |name: &str,
                                model_value: i32,
                                label: &str,
                                warnings: &mut Vec<Diagnostic>|
         -> SpiceResult<i32> {
            if !given(name) {
                return Ok(model_value);
            }
            let value = integer(instance, name)?;
            if value == 0 || value == 1 {
                return Ok(value);
            }
            warnings.push(Diagnostic::warning(
                format!("Warning: {label} has been set to its global value {model_value}."),
                "src/spicelib/devices/bsim3/b3set.c",
            ));
            Ok(model_value)
        };
        let nqs_mod = resolve_selector("nqsmod", model_nqs, "nqsMod", &mut warnings)?;
        let acnqs_mod = resolve_selector("acnqsmod", model_acnqs, "acnqsMod", &mut warnings)?;

        // Size-dependent geometry (`b3temp.c`).
        let m_lln = model_value(model, "lln")?;
        let m_lwn = model_value(model, "lwn")?;
        let m_wln = model_value(model, "wln")?;
        let m_wwn = model_value(model, "wwn")?;
        let t0 = length.powf(m_lln);
        let t1 = width.powf(m_lwn);
        let dl = model_value(model, "lint")?
            + model_value(model, "ll")? / t0
            + model_value(model, "lw")? / t1
            + model_value(model, "lwl")? / (t0 * t1);
        let dlc = model_value(model, "dlc")?
            + model_value(model, "llc")? / t0
            + model_value(model, "lwc")? / t1
            + model_value(model, "lwlc")? / (t0 * t1);
        let t2 = length.powf(m_wln);
        let t3 = width.powf(m_wwn);
        let dw = model_value(model, "wint")?
            + model_value(model, "wl")? / t2
            + model_value(model, "ww")? / t3
            + model_value(model, "wwl")? / (t2 * t3);
        let dwc = model_value(model, "dwc")?
            + model_value(model, "wlc")? / t2
            + model_value(model, "wwc")? / t3
            + model_value(model, "wwlc")? / (t2 * t3);
        let leff = length + model_value(model, "xl")? - 2.0 * dl;
        let weff = width + model_value(model, "xw")? - 2.0 * dw;
        let leff_cv = length + model_value(model, "xl")? - 2.0 * dlc;
        let weff_cv = width + model_value(model, "xw")? - 2.0 * dwc;
        for (name, value, message) in [
            ("Leff", leff, "Effective channel length <= 0"),
            ("Weff", weff, "Effective channel width <= 0"),
            ("LeffCV", leff_cv, "Effective channel length for C-V <= 0"),
            ("WeffCV", weff_cv, "Effective channel width for C-V <= 0"),
        ] {
            if !(value.is_finite() && value > 0.0) {
                return Err(SpiceError::parse(
                    location.clone(),
                    format!("BSIM3: {message} ({name} = {})", c_g(value)),
                ));
            }
        }

        let bin_unit = model_int(model, "binunit")?;
        let (inv_l, inv_w, inv_lw) = if bin_unit == 1 {
            (1.0e-6 / leff, 1.0e-6 / weff, 1.0e-12 / (leff * weff))
        } else {
            (1.0 / leff, 1.0 / weff, 1.0 / (leff * weff))
        };

        let mut p = BTreeMap::new();
        p.insert("dl", dl);
        p.insert("dw", dw);
        p.insert("dlc", dlc);
        p.insert("dwc", dwc);
        p.insert("leff", leff);
        p.insert("weff", weff);
        p.insert("leffcv", leff_cv);
        p.insert("weffcv", weff_cv);
        for &(base, l, w, area) in super::size_data::SIZE_DEPENDENT {
            let value = model_value(model, base)?
                + model_value(model, l)? * inv_l
                + model_value(model, w)? * inv_w
                + model_value(model, area)? * inv_lw;
            p.insert(base, value);
        }
        // Three C `pParam` fields are named differently from their instance
        // keywords: `BSIM3npeak` is `nch`, and `BSIM3pdibl{1,2}` are
        // `pdiblc{1,2}`. Alias them so the rest of setup can use C's names.
        for (field, keyword) in [
            ("npeak", "nch"),
            ("pdibl1", "pdiblc1"),
            ("pdibl2", "pdiblc2"),
        ] {
            let value = p.get(keyword).copied().expect("binned coefficient");
            p.insert(field, value);
        }
        p.insert(
            "abulkcvfactor",
            1.0 + (p.get("clc").copied().expect("clc") / leff_cv)
                .powf(p.get("cle").copied().expect("cle")),
        );

        let ratio =
            temperature.get("temp").expect("temperature") / temperature.get("tnom").expect("tnom");
        let t0 = ratio - 1.0;
        for (base, coefficient) in [("ua", "ua1"), ("ub", "ub1"), ("uc", "uc1")] {
            let value =
                p.get(base).copied().expect("binned") + model_value(model, coefficient)? * t0;
            p.insert(base, value);
        }
        // Three C `pParam` fields are named differently from their instance
        // keywords: `BSIM3npeak` is `nch`, and `BSIM3pdibl{1,2}` are
        // `pdiblc{1,2}`. Alias them so the rest of setup can use C's names.
        for (field, keyword) in [
            ("npeak", "nch"),
            ("pdibl1", "pdiblc1"),
            ("pdibl2", "pdiblc2"),
        ] {
            let value = p.get(keyword).copied().expect("binned coefficient");
            p.insert(field, value);
        }
        p.insert(
            "abulkcvfactor",
            1.0 + (p.get("clc").copied().expect("clc") / leff_cv)
                .powf(p.get("cle").copied().expect("cle")),
        );
        let mut u0 = p.get("u0").copied().expect("binned");
        if u0 > 1.0 {
            u0 /= 1.0e4;
            p.insert("u0", u0);
        }
        let u0temp = u0 * ratio.powf(p.get("ute").copied().expect("binned"));
        let vsattemp =
            p.get("vsat").copied().expect("binned") - p.get("at").copied().expect("binned") * t0;
        let rds0 = (p.get("rdsw").copied().expect("binned")
            + p.get("prt").copied().expect("binned") * t0)
            / (weff * METER2MICRON).powf(p.get("wr").copied().expect("binned"));
        p.insert("u0temp", u0temp);
        p.insert("vsattemp", vsattemp);
        p.insert("rds0", rds0);

        // `b3check.c`, in C order. Fatal checks accumulate before failing.
        let mut diagnostics = Vec::new();
        let mut fatal = false;
        {
            let fail = |diagnostics: &mut Vec<Diagnostic>, message: String, fatal: &mut bool| {
                *fatal = true;
                diagnostics.push(Diagnostic::fatal(
                    message,
                    "src/spicelib/devices/bsim3/b3check.c",
                ));
            };
            let version = model.version.as_str();
            if !(version.starts_with("3.3.0")
                || version.starts_with("3.30")
                || version.starts_with("3.3"))
            {
                diagnostics.push(Diagnostic::warning(
                    "Warning: This model is BSIM3v3.3.0; you specified a wrong version number.",
                    "src/spicelib/devices/bsim3/b3check.c",
                ));
            }
            macro_rules! get {
                ($name:expr) => {
                    pval(&p, model, $name)
                };
            }
            if get!("nlx") < -leff {
                fail(
                    &mut diagnostics,
                    format!("Fatal: Nlx = {} is less than -Leff.", c_g(get!("nlx"))),
                    &mut fatal,
                );
            }
            if model_value(model, "tox")? <= 0.0 {
                fail(
                    &mut diagnostics,
                    format!(
                        "Fatal: Tox = {} is not positive.",
                        c_g(model_value(model, "tox")?)
                    ),
                    &mut fatal,
                );
            }
            if model_value(model, "toxm")? <= 0.0 {
                fail(
                    &mut diagnostics,
                    format!(
                        "Fatal: Toxm = {} is not positive.",
                        c_g(model_value(model, "toxm")?)
                    ),
                    &mut fatal,
                );
            }
            if model_value(model, "lintnoi")? > leff / 2.0 {
                fail(
                    &mut diagnostics,
                    format!(
                        "Fatal: Lintnoi = {} is too large - Leff for noise is negative.",
                        c_g(model_value(model, "lintnoi")?)
                    ),
                    &mut fatal,
                );
            }
            if get!("npeak") <= 0.0 {
                fail(
                    &mut diagnostics,
                    format!("Fatal: Nch = {} is not positive.", c_g(get!("npeak"))),
                    &mut fatal,
                );
            }
            if get!("nsub") <= 0.0 {
                fail(
                    &mut diagnostics,
                    format!("Fatal: Nsub = {} is not positive.", c_g(get!("nsub"))),
                    &mut fatal,
                );
            }
            if get!("ngate") < 0.0 {
                fail(
                    &mut diagnostics,
                    format!("Fatal: Ngate = {} is not positive.", c_g(get!("ngate"))),
                    &mut fatal,
                );
            }
            if get!("ngate") > 1.0e25 {
                fail(
                    &mut diagnostics,
                    format!("Fatal: Ngate = {} is too high.", c_g(get!("ngate"))),
                    &mut fatal,
                );
            }
            if get!("xj") <= 0.0 {
                fail(
                    &mut diagnostics,
                    format!("Fatal: Xj = {} is not positive.", c_g(get!("xj"))),
                    &mut fatal,
                );
            }
            if get!("dvt1") < 0.0 {
                fail(
                    &mut diagnostics,
                    format!("Fatal: Dvt1 = {} is negative.", c_g(get!("dvt1"))),
                    &mut fatal,
                );
            }
            if get!("dvt1w") < 0.0 {
                fail(
                    &mut diagnostics,
                    format!("Fatal: Dvt1w = {} is negative.", c_g(get!("dvt1w"))),
                    &mut fatal,
                );
            }
            if get!("w0") == -weff {
                fail(
                    &mut diagnostics,
                    "Fatal: (W0 + Weff) = 0 causing divided-by-zero.".into(),
                    &mut fatal,
                );
            }
            if get!("dsub") < 0.0 {
                fail(
                    &mut diagnostics,
                    format!("Fatal: Dsub = {} is negative.", c_g(get!("dsub"))),
                    &mut fatal,
                );
            }
            if get!("b1") == -weff {
                fail(
                    &mut diagnostics,
                    "Fatal: (B1 + Weff) = 0 causing divided-by-zero.".into(),
                    &mut fatal,
                );
            }
            if get!("u0temp") <= 0.0 {
                fail(
                    &mut diagnostics,
                    format!(
                        "Fatal: u0 at current temperature = {} is not positive.",
                        c_g(get!("u0temp"))
                    ),
                    &mut fatal,
                );
            }
            if get!("delta") < 0.0 {
                fail(
                    &mut diagnostics,
                    format!("Fatal: Delta = {} is less than zero.", c_g(get!("delta"))),
                    &mut fatal,
                );
            }
            if get!("vsattemp") <= 0.0 {
                fail(
                    &mut diagnostics,
                    format!(
                        "Fatal: Vsat at current temperature = {} is not positive.",
                        c_g(get!("vsattemp"))
                    ),
                    &mut fatal,
                );
            }
            if get!("pclm") <= 0.0 {
                fail(
                    &mut diagnostics,
                    format!("Fatal: Pclm = {} is not positive.", c_g(get!("pclm"))),
                    &mut fatal,
                );
            }
            if get!("drout") < 0.0 {
                fail(
                    &mut diagnostics,
                    format!("Fatal: Drout = {} is negative.", c_g(get!("drout"))),
                    &mut fatal,
                );
            }
            if get!("pscbe2") <= 0.0 {
                diagnostics.push(Diagnostic::warning(
                    format!("Warning: Pscbe2 = {} is not positive.", c_g(get!("pscbe2"))),
                    "src/spicelib/devices/bsim3/b3check.c",
                ));
            }
            if model_value(model, "cjsw")? > 0.0 || model_value(model, "cjswg")? > 0.0 {
                if drain_perimeter < weff {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Pd = {} is less than W.", c_g(drain_perimeter)),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if source_perimeter < weff {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Ps = {} is less than W.", c_g(source_perimeter)),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
            }
            if get!("noff") < 0.1 {
                diagnostics.push(Diagnostic::warning(
                    format!("Warning: Noff = {} is too small.", c_g(get!("noff"))),
                    "src/spicelib/devices/bsim3/b3check.c",
                ));
            }
            if get!("noff") > 4.0 {
                diagnostics.push(Diagnostic::warning(
                    format!("Warning: Noff = {} is too large.", c_g(get!("noff"))),
                    "src/spicelib/devices/bsim3/b3check.c",
                ));
            }
            if get!("voffcv") < -0.5 {
                diagnostics.push(Diagnostic::warning(
                    format!("Warning: Voffcv = {} is too small.", c_g(get!("voffcv"))),
                    "src/spicelib/devices/bsim3/b3check.c",
                ));
            }
            if get!("voffcv") > 0.5 {
                diagnostics.push(Diagnostic::warning(
                    format!("Warning: Voffcv = {} is too large.", c_g(get!("voffcv"))),
                    "src/spicelib/devices/bsim3/b3check.c",
                ));
            }
            if model_value(model, "ijth")? < 0.0 {
                fail(
                    &mut diagnostics,
                    format!(
                        "Fatal: Ijth = {} cannot be negative.",
                        c_g(model_value(model, "ijth")?)
                    ),
                    &mut fatal,
                );
            }
            if get!("clc") < 0.0 {
                fail(
                    &mut diagnostics,
                    format!("Fatal: Clc = {} is negative.", c_g(get!("clc"))),
                    &mut fatal,
                );
            }
            if get!("moin") < 5.0 {
                diagnostics.push(Diagnostic::warning(
                    format!("Warning: Moin = {} is too small.", c_g(get!("moin"))),
                    "src/spicelib/devices/bsim3/b3check.c",
                ));
            }
            if get!("moin") > 25.0 {
                diagnostics.push(Diagnostic::warning(
                    format!("Warning: Moin = {} is too large.", c_g(get!("moin"))),
                    "src/spicelib/devices/bsim3/b3check.c",
                ));
            }
            if model_int(model, "capmod")? == 3 {
                if get!("acde") < 0.4 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning:  Acde = {} is too small.", c_g(get!("acde"))),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if get!("acde") > 1.6 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning:  Acde = {} is too large.", c_g(get!("acde"))),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
            }
            if model_int(model, "paramchk")? == 1 {
                if get!("leff") <= 5.0e-8 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Leff = {} may be too small.", c_g(get!("leff"))),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if get!("leffcv") <= 5.0e-8 {
                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "Warning: Leff for CV = {} may be too small.",
                            c_g(get!("leffcv"))
                        ),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if get!("weff") <= 1.0e-7 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Weff = {} may be too small.", c_g(get!("weff"))),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if get!("weffcv") <= 1.0e-7 {
                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "Warning: Weff for CV = {} may be too small.",
                            c_g(get!("weffcv"))
                        ),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if get!("nlx") < 0.0 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Nlx = {} is negative.", c_g(get!("nlx"))),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if model_value(model, "tox")? < 1.0e-9 {
                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "Warning: Tox = {} is less than 10A.",
                            c_g(model_value(model, "tox")?)
                        ),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                let npeak = get!("npeak");
                if npeak <= 1.0e15 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Nch = {} may be too small.", c_g(npeak)),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                } else if npeak >= 1.0e21 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Nch = {} may be too large.", c_g(npeak)),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                let nsub = get!("nsub");
                if nsub <= 1.0e14 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Nsub = {} may be too small.", c_g(nsub)),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                } else if nsub >= 1.0e21 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Nsub = {} may be too large.", c_g(nsub)),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                let ngate = get!("ngate");
                if ngate > 0.0 && ngate <= 1.0e18 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Ngate = {} is less than 1.E18cm^-3.", c_g(ngate)),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if get!("dvt0") < 0.0 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Dvt0 = {} is negative.", c_g(get!("dvt0"))),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if (1.0e-6 / (get!("w0") + weff)).abs() > 10.0 {
                    diagnostics.push(Diagnostic::warning(
                        "Warning: (W0 + Weff) may be too small.",
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if get!("nfactor") < 0.0 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Nfactor = {} is negative.", c_g(get!("nfactor"))),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if get!("cdsc") < 0.0 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Cdsc = {} is negative.", c_g(get!("cdsc"))),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if get!("cdscd") < 0.0 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Cdscd = {} is negative.", c_g(get!("cdscd"))),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if get!("eta0") < 0.0 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Eta0 = {} is negative.", c_g(get!("eta0"))),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if (1.0e-6 / (get!("b1") + weff)).abs() > 10.0 {
                    diagnostics.push(Diagnostic::warning(
                        "Warning: (B1 + Weff) may be too small.",
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                let a2 = get!("a2");
                if a2 < 0.01 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: A2 = {} is too small. Set to 0.01.", c_g(a2)),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                    p.insert("a2", 0.01);
                } else if a2 > 1.0 {
                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "Warning: A2 = {} is larger than 1. A2 is set to 1 and A1 is set to 0.",
                            c_g(a2)
                        ),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                    p.insert("a2", 1.0);
                    p.insert("a1", 0.0);
                }
                let rdsw = get!("rdsw");
                if rdsw < 0.0 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Rdsw = {} is negative. Set to zero.", c_g(rdsw)),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                    p.insert("rdsw", 0.0);
                    p.insert("rds0", 0.0);
                } else if get!("rds0") > 0.0 && get!("rds0") < 0.001 {
                    diagnostics.push(Diagnostic::warning(format!("Warning: Rds at current temperature = {} is less than 0.001 ohm. Set to zero.", c_g(get!("rds0"))), "src/spicelib/devices/bsim3/b3check.c"));
                    p.insert("rds0", 0.0);
                }
                if get!("vsattemp") < 1.0e3 {
                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "Warning: Vsat at current temperature = {} may be too small.",
                            c_g(get!("vsattemp"))
                        ),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if get!("pdibl1") < 0.0 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Pdibl1 = {} is negative.", c_g(get!("pdibl1"))),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
                if get!("pdibl2") < 0.0 {
                    diagnostics.push(Diagnostic::warning(
                        format!("Warning: Pdibl2 = {} is negative.", c_g(get!("pdibl2"))),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                }
            }
        }
        // The CGDO/CGSO/CGBO resets belong to the `paramchk` block in C and
        // persist in the shared model.
        if model_int(model, "paramchk")? == 1 {
            let reset = |value: &mut Real, name: &str, diagnostics: &mut Vec<Diagnostic>| {
                if *value < 0.0 {
                    diagnostics.push(Diagnostic::warning(
                        format!(
                            "Warning: {name} = {} is negative. Set to zero.",
                            c_g(*value)
                        ),
                        "src/spicelib/devices/bsim3/b3check.c",
                    ));
                    *value = 0.0;
                }
            };
            reset(model_cgdo, "cgdo", &mut diagnostics);
            reset(model_cgso, "cgso", &mut diagnostics);
            reset(model_cgbo, "cgbo", &mut diagnostics);
        }
        if fatal {
            let messages: Vec<_> = diagnostics
                .iter()
                .filter(|d| d.fatal)
                .map(|d| d.message.as_str())
                .collect();
            return Err(SpiceError::parse(
                location.clone(),
                format!(
                    "Fatal error(s) detected during BSIM3V3.3 parameter checking: {}",
                    messages.join(" ")
                ),
            ));
        }
        warnings.append(&mut diagnostics);

        // Overlap capacitances and C's `tconst` (see the quirk note above).
        let cox = model.oxide_capacitance;
        let cf = p.get("cf").copied().expect("binned");
        p.insert("cgdo", (*model_cgdo + cf) * weff_cv);
        p.insert("cgso", (*model_cgso + cf) * weff_cv);
        p.insert("cgbo", *model_cgbo * leff_cv);
        let elm = p.get("elm").copied().expect("binned");
        let mut tconst_quirk = u0temp * elm / (cox * weff_cv * leff_cv * (leff_cv * leff_cv));
        p.insert("tconst", tconst_quirk);

        // Potential and threshold derivations.
        let vtm0 = temperature.get("vtm0").expect("vtm0");
        let ni = temperature.get("ni").expect("ni");
        if !model.is_given("nch") && model.is_given("gamma1") {
            let gamma1 = p.get("gamma1").copied().expect("binned");
            let t0 = gamma1 * cox;
            p.insert("npeak", 3.021e22 * t0 * t0);
        }
        let npeak = p.get("npeak").copied().expect("npeak");
        let phi = 2.0 * vtm0 * (npeak / ni).ln();
        let sqrt_phi = phi.sqrt();
        p.insert("phi", phi);
        p.insert("sqrtphi", sqrt_phi);
        p.insert("phis3", sqrt_phi * phi);
        let eps_si = 1.035_94e-10;
        let charge_q = 1.602_19e-19;
        let xdep0 = (2.0 * eps_si / (charge_q * npeak * 1.0e6)).sqrt() * sqrt_phi;
        p.insert("xdep0", xdep0);
        p.insert("sqrtxdep0", xdep0.sqrt());
        p.insert(
            "litl",
            (3.0 * p.get("xj").copied().expect("xj") * model_value(model, "tox")?).sqrt(),
        );
        p.insert("vbi", vtm0 * (1.0e20 * npeak / (ni * ni)).ln());
        p.insert(
            "cdep0",
            (charge_q * eps_si * npeak * 1.0e6 / 2.0 / phi).sqrt(),
        );
        p.insert(
            "ldeb",
            (eps_si * vtm0 / (charge_q * npeak * 1.0e6)).sqrt() / 3.0,
        );
        let acde = p.get("acde").copied().expect("acde") * (npeak / 2.0e16).powf(-0.25);
        p.insert("acde", acde);

        // K1/K2: explicit pair, or derived from doping and flat-band data.
        let warn = |message: String, warnings: &mut Vec<Diagnostic>| {
            warnings.push(Diagnostic::warning(
                message,
                "src/spicelib/devices/bsim3/b3temp.c",
            ));
        };
        if model.is_given("k1") || model.is_given("k2") {
            if !model.is_given("k1") {
                warn(
                    "Warning: k1 should be specified with k2.".into(),
                    &mut warnings,
                );
                p.insert("k1", 0.53);
            }
            if !model.is_given("k2") {
                warn(
                    "Warning: k2 should be specified with k1.".into(),
                    &mut warnings,
                );
                p.insert("k2", -0.0186);
            }
            for name in ["nsub", "xt", "vbx", "gamma1", "gamma2"] {
                if model.is_given(name) {
                    warn(
                        format!("Warning: {name} is ignored because k1 or k2 is given."),
                        &mut warnings,
                    );
                }
            }
        } else {
            if !model.is_given("vbx") {
                let vbx = phi - 7.7348e-4 * npeak * p.get("xt").copied().expect("xt").powi(2);
                p.insert("vbx", vbx);
            }
            if p.get("vbx").copied().expect("vbx") > 0.0 {
                let value = -p.get("vbx").copied().expect("vbx");
                p.insert("vbx", value);
            }
            if p.get("vbm").copied().expect("vbm") > 0.0 {
                let value = -p.get("vbm").copied().expect("vbm");
                p.insert("vbm", value);
            }
            if !model.is_given("gamma1") {
                p.insert("gamma1", 5.753e-12 * npeak.sqrt() / cox);
            }
            if !model.is_given("gamma2") {
                p.insert(
                    "gamma2",
                    5.753e-12 * p.get("nsub").copied().expect("nsub").sqrt() / cox,
                );
            }
            let gamma1 = p.get("gamma1").copied().expect("gamma1");
            let gamma2 = p.get("gamma2").copied().expect("gamma2");
            let vbx = p.get("vbx").copied().expect("vbx");
            let vbm = p.get("vbm").copied().expect("vbm");
            let t0 = gamma1 - gamma2;
            let t1 = (phi - vbx).sqrt() - sqrt_phi;
            let t2 = (phi * (phi - vbm)).sqrt() - phi;
            let k2 = t0 * t1 / (2.0 * t2 + vbm);
            let k1 = gamma2 - 2.0 * k2 * (phi - vbm).sqrt();
            p.insert("k2", k2);
            p.insert("k1", k1);
        }

        let k1 = p.get("k1").copied().expect("k1");
        let k2 = p.get("k2").copied().expect("k2");
        let vbm = p.get("vbm").copied().expect("vbm");
        let vbsc = if k2 < 0.0 {
            let t0 = 0.5 * k1 / k2;
            // `b3temp.c` clamps to [-30, -3]; endpoint values are unchanged.
            (0.9 * (phi - t0 * t0)).clamp(-30.0, -3.0)
        } else {
            -30.0
        };
        let vbsc = if vbsc > vbm { vbm } else { vbsc };
        p.insert("vbsc", vbsc);

        let polarity = Real::from(model.polarity);
        if !model.is_given("vfb") {
            let vfb = if model.is_given("vth0") {
                polarity * p.get("vth0").copied().expect("vth0") - phi - k1 * sqrt_phi
            } else {
                -1.0
            };
            p.insert("vfb", vfb);
        }
        if !model.is_given("vth0") {
            let vfb = p.get("vfb").copied().expect("vfb");
            p.insert("vth0", polarity * (vfb + phi + k1 * sqrt_phi));
        }
        let k1ox = k1 * model_value(model, "tox")? / model_value(model, "toxm")?;
        let k2ox = k2 * model_value(model, "tox")? / model_value(model, "toxm")?;
        p.insert("k1ox", k1ox);
        p.insert("k2ox", k2ox);

        let factor1 = temperature.get("factor1").expect("factor1");
        let dsub = p.get("dsub").copied().expect("dsub");
        let t1 = (eps_si / 3.453_133e-11 * model_value(model, "tox")? * xdep0).sqrt();
        let t0 = (-0.5 * dsub * leff / t1).exp();
        p.insert("theta0vb0", t0 + 2.0 * t0 * t0);
        let t0 = (-0.5 * p.get("drout").copied().expect("drout") * leff / t1).exp();
        let t2 = t0 + 2.0 * t0 * t0;
        p.insert(
            "thetarout",
            p.get("pdibl1").copied().expect("pdibl1") * t2
                + p.get("pdibl2").copied().expect("pdibl2"),
        );

        let tmp = xdep0.sqrt();
        let tmp1 = p.get("vbi").copied().expect("vbi") - phi;
        let tmp2 = factor1 * tmp;
        let t0 = -0.5 * p.get("dvt1w").copied().expect("dvt1w") * weff * leff / tmp2;
        let t2 = if t0 > -EXP_THRESHOLD {
            let e = t0.exp();
            e * (1.0 + 2.0 * e)
        } else {
            MIN_EXP * (1.0 + 2.0 * MIN_EXP)
        };
        let t0 = p.get("dvt0w").copied().expect("dvt0w") * t2;
        let t2w = t0 * tmp1;
        let t0 = -0.5 * p.get("dvt1").copied().expect("dvt1") * leff / tmp2;
        let t3 = if t0 > -EXP_THRESHOLD {
            let e = t0.exp();
            e * (1.0 + 2.0 * e)
        } else {
            MIN_EXP * (1.0 + 2.0 * MIN_EXP)
        };
        let t3 = p.get("dvt0").copied().expect("dvt0") * t3 * tmp1;
        let t4 = model_value(model, "tox")? * phi / (weff + p.get("w0").copied().expect("w0"));
        let nlx = p.get("nlx").copied().expect("nlx");
        let t0 = (1.0 + nlx / leff).sqrt();
        let t5 = k1ox * (t0 - 1.0) * sqrt_phi
            + (p.get("kt1").copied().expect("kt1") + p.get("kt1l").copied().expect("kt1l") / leff)
                * (ratio - 1.0);
        let tmp3 = polarity * p.get("vth0").copied().expect("vth0") - t2w - t3
            + p.get("k3").copied().expect("k3") * t4
            + t5;
        p.insert("vfbzb", tmp3 - phi - k1 * sqrt_phi);

        for name in [
            "phi",
            "sqrtphi",
            "phis3",
            "xdep0",
            "sqrtxdep0",
            "litl",
            "vbi",
            "cdep0",
            "ldeb",
            "acde",
            "k1",
            "k2",
            "k1ox",
            "k2ox",
            "vbsc",
            "vfb",
            "vth0",
            "vfbzb",
            "theta0vb0",
            "thetarout",
            "u0temp",
            "vsattemp",
            "rds0",
            "cgdo",
            "cgso",
            "cgbo",
            "tconst",
            "abulkcvfactor",
        ] {
            if let Some(value) = p.get(name)
                && !value.is_finite()
            {
                return Err(SpiceError::Numerical {
                    context: "BSIM3 size-dependent setup (bsim3/b3temp.c)".into(),
                    message: format!("{name} is not finite"),
                });
            }
        }

        // Instance-level adjustments: DELVTO and MULU0.
        let vth0 = p.get("vth0").copied().expect("vth0") + delvto;
        let vfb = p.get("vfb").copied().expect("vfb") + polarity * delvto;
        let vfbzb = p.get("vfbzb").copied().expect("vfbzb") + polarity * delvto;
        let u0temp_instance = p.get("u0temp").copied().expect("u0temp") * mulu0;
        // C recomputes `tconst` here with `T0 = sqrt(1 + nlx/leff)` from above.
        tconst_quirk = u0temp_instance * elm / (cox * weff_cv * leff_cv * t0);
        p.insert("tconst", tconst_quirk);

        // Series resistances (ACM off): sheet resistance times squares.
        let sheet_resistance = model_value(model, "rsh")?;
        let drain_resistance = sheet_resistance * drain_squares;
        let source_resistance = sheet_resistance * source_squares;
        let drain_conductance = if drain_resistance > 0.0 {
            1.0 / drain_resistance
        } else {
            0.0
        };
        let source_conductance = if source_resistance > 0.0 {
            1.0 / source_resistance
        } else {
            0.0
        };

        // Junction saturation currents and breakdown voltages.
        let vtm = temperature.get("vtm").expect("vtm");
        let nvtm = vtm * model_value(model, "nj")?;
        let jct_temp = temperature
            .get("jct_temp_sat_cur_density")
            .expect("junction current");
        let jct_sidewall_temp = temperature
            .get("jct_sidewall_temp_sat_cur_density")
            .expect("junction current");
        let ijth = model_value(model, "ijth")?;
        let source_sat_current = if source_area <= 0.0 && source_perimeter <= 0.0 {
            1.0e-14
        } else {
            source_area * jct_temp + source_perimeter * jct_sidewall_temp
        };
        let drain_sat_current = if drain_area <= 0.0 && drain_perimeter <= 0.0 {
            1.0e-14
        } else {
            drain_area * jct_temp + drain_perimeter * jct_sidewall_temp
        };
        let (mut vjsm, mut isevjsm) = (0.0, 0.0);
        if source_sat_current > 0.0 && ijth > 0.0 {
            vjsm = nvtm * (ijth / source_sat_current + 1.0).ln();
            isevjsm = source_sat_current * (vjsm / nvtm).exp();
        }
        let (mut vjdm, mut isevjdm) = (0.0, 0.0);
        if drain_sat_current > 0.0 && ijth > 0.0 {
            vjdm = nvtm * (ijth / drain_sat_current + 1.0).ln();
            isevjdm = drain_sat_current * (vjdm / nvtm).exp();
        }
        let setup = Self {
            length,
            width,
            leff,
            weff,
            leff_cv,
            weff_cv,
            polarity: model.polarity,
            multiplier,
            nqs_mod,
            acnqs_mod,
            geo,
            drain_area,
            source_area,
            drain_perimeter,
            source_perimeter,
            drain_squares,
            source_squares,
            delvto,
            mulu0,
            ic_vbs,
            ic_vds,
            ic_vgs,
            vth0,
            vfb,
            vfbzb,
            u0temp: u0temp_instance,
            tconst: tconst_quirk,
            cgso: p.get("cgso").copied().expect("cgso"),
            cgdo: p.get("cgdo").copied().expect("cgdo"),
            drain_conductance,
            source_conductance,
            source_sat_current,
            drain_sat_current,
            vjsm,
            vjdm,
            isevjsm,
            isevjdm,
            pparam: p,
            warnings,
        };
        Ok(setup)
    }
}

/// A `b3temp.c` size-dependent parameter, falling back to the model field for
/// names that are not binned.
fn pval(p: &BTreeMap<&'static str, Real>, model: &ModelDefaults, name: &str) -> Real {
    p.get(name)
        .copied()
        .or_else(|| model.get(name))
        .unwrap_or_else(|| panic!("BSIM3 setup field {name} is missing"))
}

fn model_int(model: &ModelDefaults, name: &str) -> SpiceResult<i32> {
    Ok(model_value(model, name)? as i32)
}

fn model_value(model: &ModelDefaults, name: &str) -> SpiceResult<Real> {
    model
        .get(name)
        .ok_or_else(|| SpiceError::circuit(format!("BSIM3 model defaults lost {name}")))
}

fn integer(inputs: &Inputs, name: &str) -> SpiceResult<i32> {
    let Some(applied) = inputs.get(name) else {
        return Err(SpiceError::circuit(format!(
            "BSIM3 setup needs defaulted {name}"
        )));
    };
    match &applied.value {
        Value::Integer(value) => Ok(*value),
        Value::Real(value) => Ok(*value as i32),
        value => Err(SpiceError::circuit(format!(
            "BSIM3 setup expected integer {name}, found {value:?}"
        ))),
    }
}

fn instance_real(inputs: &Inputs, name: &str) -> SpiceResult<Real> {
    let Some(applied) = inputs.get(name) else {
        return Err(SpiceError::circuit(format!(
            "BSIM3 instance setup expected {name}"
        )));
    };
    match &applied.value {
        Value::Real(value) => Ok(*value),
        Value::Integer(value) => Ok(Real::from(*value)),
        value => Err(SpiceError::parse(
            applied.location.clone(),
            format!("BSIM3 instance {name} is not numeric: {value:?}"),
        )),
    }
}
