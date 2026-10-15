//! The macspice MESA/HFET family, `Z` levels 2–6.
//!
//! `mesa/`, `hfet1/` and `hfet2/` are three copies of one lineage: the same
//! `SPICEdev` shape, the same conditional internal nodes and the same
//! temperature/truncation/ask/pole-zero split. This module owns what they
//! share; one file per level holds its channel.
//!
//! C references, read as behaviour only: `parser/inp2z.c` and
//! `parser/inpdomod.c` for the card and the level map (0 or 1 `mes/`, 2–4
//! `mesa/`, 5 `hfet1/`, 6 `hfet2/`); `mesa/mesa.c` for `MESApTable` and
//! `MESAmPTable`; `mesa/mesaparam.c` and `mesamparam.c` for the setters;
//! `mesa/mesasetup.c` for the defaults and the internal nodes; and
//! `mesa/mesatemp.c` for the derived quantities.
//!
//! # C quirks reproduced deliberately
//!
//! - **MESA is NMF only.** `MESAsetup` prints "Only nmf model type supported,
//!   set to nmf" and forces the type for a `pmf` model, which then simulates
//!   as an N-channel device (checked against the reference binary). The port
//!   does not carry a polarity factor for this family at all, as C does not.
//! - **`td`/`ts` are inconsistent.** An explicit `td`/`ts` is Celsius and C
//!   stores `value + CONSTCtoK`; the *default* is `CKTtemp + dtemp`, i.e. a
//!   Celsius value used directly as kelvin by `mesatemp.c`. Both are
//!   reproduced, because the defaults feed `csatfs`, `vcrits` and every
//!   temperature coefficient.
//! - **`tf` defaults to `CKTtemp`** in kelvin, and `mesaacl.c` blends
//!   `tlambda` towards `tLambdahf` with `tanh((f - fl)/delf)`; with the
//!   default `delfo` of zero that blend is inactive.
//!
//! # Not ported
//!
//! `mesaacl.c`, `mesapzl.c`, `mesatrunc.c` and the charge storage of
//! `mesaload.c` belong to H1b; level 2 answers `.op` and `.dc` only and
//! refuses transient, AC, pole-zero and the `ri`/`rf`/`rdi`/`rsi` paths.

pub(crate) mod mesa;

use crate::devices::jfet::{self, p};
use crate::devices::schema::{
    ScalarDomain as D, ScalarParameter as P, ScalarSchema, ScalarUnit as U,
};
use crate::devices::{ModelFamily, ResolvedModel};
use crate::primitives::{Real, SpiceError, SpiceResult};

/// Model setters of `MESAmPTable` with `mesasetup.c`'s defaults. `vto` is the
/// cardinal spelling and `vt0` its `IOPR` alias; `phib1`/`tphib`,
/// `xchi`/`tggr` and `tvto`/`alphat` are alias pairs of one C parameter, so
/// their defaults are absent here and resolved by [`Model::resolve`].
const MODEL: &[P] = &[
    p("vto", U::Volt, D::Finite, None),
    p("vt0", U::Volt, D::Finite, None),
    p("lambda", U::InverseVolt, D::NonNegative, Some(0.045)),
    p("lambdahf", U::InverseVolt, D::NonNegative, None),
    p("beta", U::AmperePerVoltSquared, D::Finite, Some(8.5e-3)),
    p("vs", U::MetrePerSecond, D::Positive, Some(1.5e5)),
    p("rd", U::Ohm, D::NonNegative, Some(0.)),
    p("rs", U::Ohm, D::NonNegative, Some(0.)),
    p("rg", U::Ohm, D::NonNegative, Some(0.)),
    p("ri", U::Ohm, D::NonNegative, Some(0.)),
    p("rf", U::Ohm, D::NonNegative, Some(0.)),
    p("rdi", U::Ohm, D::NonNegative, Some(0.)),
    p("rsi", U::Ohm, D::NonNegative, Some(0.)),
    p("phib", U::Volt, D::Finite, Some(0.5 * 1.602_176_634e-19)),
    p("phib1", U::VoltPerKelvin, D::Finite, None),
    p("tphib", U::VoltPerKelvin, D::Finite, None),
    p("astar", U::Dimensionless, D::NonNegative, Some(4.0e4)),
    p("ggr", U::SiemensPerMetre, D::NonNegative, Some(40.)),
    p("del", U::Dimensionless, D::NonNegative, Some(0.04)),
    p("xchi", U::InverseKelvin, D::Finite, None),
    p("tggr", U::InverseKelvin, D::Finite, None),
    p("n", U::Dimensionless, D::Finite, Some(1.)),
    p("eta", U::Dimensionless, D::Finite, Some(1.73)),
    p("m", U::Dimensionless, D::Finite, Some(2.5)),
    p("mc", U::Dimensionless, D::Finite, Some(3.0)),
    p("alpha", U::Dimensionless, D::Finite, Some(0.)),
    p("sigma0", U::Dimensionless, D::Finite, Some(0.081)),
    p("vsigmat", U::Volt, D::Finite, Some(1.01)),
    p("vsigma", U::Volt, D::Positive, Some(0.1)),
    p(
        "mu",
        U::SquareCentimetrePerVoltSecond,
        D::NonNegative,
        Some(0.23),
    ),
    p("theta", U::Dimensionless, D::Finite, Some(0.)),
    p(
        "mu1",
        U::SquareCentimetrePerVoltSecond,
        D::NonNegative,
        Some(0.),
    ),
    p(
        "mu2",
        U::SquareCentimetrePerVoltSecond,
        D::NonNegative,
        Some(0.),
    ),
    p("d", U::Metre, D::Positive, Some(0.12e-6)),
    p("nd", U::PerCubicCentimetre, D::Positive, Some(2.0e23)),
    p("du", U::Metre, D::Positive, Some(0.035e-6)),
    p("ndu", U::PerCubicCentimetre, D::Positive, Some(1e22)),
    p("th", U::Metre, D::NonNegative, Some(0.01e-6)),
    p("ndelta", U::PerCubicCentimetre, D::NonNegative, Some(6e24)),
    p("delta", U::Dimensionless, D::Finite, Some(5.0)),
    p("tc", U::InverseVolt, D::Finite, Some(0.)),
    p("tvto", U::VoltPerKelvin, D::Finite, None),
    p("alphat", U::VoltPerKelvin, D::Finite, None),
    p("tlambda", U::Kelvin, D::Finite, Some(Real::MAX)),
    p("teta0", U::Kelvin, D::Finite, Some(Real::MAX)),
    p("teta1", U::Kelvin, D::Finite, Some(0.)),
    p("tmu", U::Kelvin, D::Positive, Some(300.15)),
    p("xtm0", U::Dimensionless, D::Finite, Some(0.)),
    p("xtm1", U::Dimensionless, D::Finite, Some(0.)),
    p("xtm2", U::Dimensionless, D::Finite, Some(0.)),
    p("ks", U::Dimensionless, D::Finite, Some(0.)),
    p("vsg", U::Volt, D::Finite, Some(0.)),
    p("tf", U::Kelvin, D::NonZero, None),
    p("flo", U::Dimensionless, D::Finite, Some(0.)),
    p("delfo", U::Dimensionless, D::Finite, Some(0.)),
    p("ag", U::Dimensionless, D::Finite, Some(0.)),
    p("rtc1", U::InverseKelvin, D::Finite, Some(0.)),
    p("rtc2", U::InverseKelvinSquared, D::Finite, Some(0.)),
    p("zeta", U::Dimensionless, D::Finite, Some(1.)),
    p("level", U::Dimensionless, D::Finite, Some(2.)),
    p("nmax", U::Dimensionless, D::NonNegative, Some(2e16)),
    p("gamma", U::Dimensionless, D::Finite, Some(3.0)),
    p(
        "epsi",
        U::FaradPerMetre,
        D::Positive,
        Some(12.244 * 8.854_18e-12),
    ),
    p("cas", U::Dimensionless, D::Finite, Some(1.)),
    p("cbs", U::Dimensionless, D::Finite, Some(1.)),
];

/// Scalar instance setters of `MESApTable` (`mesasetup.c` and
/// `mesaparam.c`); `off` and `icvds`/`icvgs` are split off first. MESA has no
/// `area`.
const INSTANCE: &[P] = &[
    p("l", U::Metre, D::Positive, Some(1e-6)),
    p("w", U::Metre, D::Positive, Some(20e-6)),
    p("m", U::Dimensionless, D::Positive, Some(1.)),
    p("td", U::Celsius, D::Temperature, None),
    p("ts", U::Celsius, D::Temperature, None),
    p("dtemp", U::Dimensionless, D::Finite, Some(0.)),
];

/// The `IC` vector components (`mesaparam.c`): `ICVDS`, then `ICVGS`.
pub(super) const IC_COMPONENTS: [&str; 2] = ["icvds", "icvgs"];

/// Validated model card of a level-2 MESA device.
///
/// Only the setters `mesa1` and its temperature derivations read are stored.
/// `mesasetup.c`/`mesamparam.c` still accept the rest — they belong to the
/// level-3/4 channels (`mesa2`, `mesa3`) — so the schema validates them and C
/// ignores them at level 2 exactly as this port does: `beta`, `ndu`,
/// `ndelta`, `nmax`, `gamma`, `epsi`, `cas` and `cbs`. The [H1
/// reference notes](https://github.com/michaelnavazhylau/ngspice-rs/blob/main/docs/port/M10_SPILL_PLAN.md)
/// say which channel reads which.
#[derive(Debug, Clone, Copy)]
pub(super) struct Model {
    pub(super) threshold: Real,
    pub(super) lambda: Real,
    /// `mesatemp.c` uses the model's `lambda` when `lambdahf` is unset, and
    /// `mesaacl.c` blends the two through the `fl`/`delf` window.
    pub(super) lambdahf: Option<Real>,
    pub(super) vs: Real,
    pub(super) rd: Real,
    pub(super) rs: Real,
    pub(super) rg: Real,
    pub(super) ri: Real,
    pub(super) rf: Real,
    pub(super) rdi: Real,
    pub(super) rsi: Real,
    pub(super) phib: Real,
    pub(super) phib1: Real,
    pub(super) astar: Real,
    pub(super) ggr: Real,
    pub(super) del: Real,
    pub(super) xchi: Real,
    pub(super) n: Real,
    pub(super) eta: Real,
    pub(super) m: Real,
    pub(super) mc: Real,
    pub(super) alpha: Real,
    pub(super) sigma0: Real,
    pub(super) vsigmat: Real,
    pub(super) vsigma: Real,
    pub(super) mu: Real,
    pub(super) theta: Real,
    pub(super) mu1: Real,
    pub(super) mu2: Real,
    pub(super) d: Real,
    pub(super) nd: Real,
    pub(super) du: Real,
    pub(super) th: Real,
    pub(super) delta: Real,
    pub(super) tc: Real,
    pub(super) tvto: Real,
    pub(super) tlambda: Real,
    pub(super) teta0: Real,
    pub(super) teta1: Real,
    pub(super) tmu: Real,
    pub(super) xtm0: Real,
    pub(super) xtm1: Real,
    pub(super) xtm2: Real,
    pub(super) ks: Real,
    pub(super) vsg: Real,
    /// `mesasetup.c`'s `CKTtemp` default (kelvin) and the `mesaacl.c`
    /// frequency blend of `tlambda` with `tLambdahf`.
    pub(super) tf: Option<Real>,
    pub(super) flo: Real,
    pub(super) delfo: Real,
    pub(super) ag: Real,
    pub(super) tc1: Real,
    pub(super) tc2: Real,
    pub(super) zeta: Real,
}

impl Model {
    /// `MESAsetup`'s defaults, `mesamparam.c`'s setters and `inpdomod.c`'s
    /// level check. The `pmf` flag is accepted and ignored: C forces `NMF`
    /// with a warning, and the port reproduces the model rather than the
    /// message.
    pub(super) fn resolve(resolved: &ResolvedModel<'_>) -> SpiceResult<Self> {
        let (_, scalars) = jfet::typed_scalars(
            resolved,
            resolved.family() == ModelFamily::Pmf,
            ["nmf", "pmf"],
        );
        let values =
            ScalarSchema { parameters: MODEL }.validate(scalars, &resolved.card().location)?;
        let r = |name: &str| jfet::required(&values, name);
        let alias = |names: &[&str]| jfet::last_setter(&values, names);
        let model = Self {
            threshold: alias(&["vto", "vt0"]).unwrap_or(-1.26),
            lambda: r("lambda")?,
            lambdahf: alias(&["lambdahf"]),
            vs: r("vs")?,
            rd: r("rd")?,
            rs: r("rs")?,
            rg: r("rg")?,
            ri: r("ri")?,
            rf: r("rf")?,
            rdi: r("rdi")?,
            rsi: r("rsi")?,
            phib: r("phib")?,
            phib1: alias(&["phib1", "tphib"]).unwrap_or(0.),
            astar: r("astar")?,
            ggr: r("ggr")?,
            del: r("del")?,
            xchi: alias(&["xchi", "tggr"]).unwrap_or(0.033),
            n: r("n")?,
            eta: r("eta")?,
            m: r("m")?,
            mc: r("mc")?,
            alpha: r("alpha")?,
            sigma0: r("sigma0")?,
            vsigmat: r("vsigmat")?,
            vsigma: r("vsigma")?,
            mu: r("mu")?,
            theta: r("theta")?,
            mu1: r("mu1")?,
            mu2: r("mu2")?,
            d: r("d")?,
            nd: r("nd")?,
            du: r("du")?,
            th: r("th")?,
            delta: r("delta")?,
            tc: r("tc")?,
            tvto: alias(&["tvto", "alphat"]).unwrap_or(0.),
            tlambda: r("tlambda")?,
            teta0: r("teta0")?,
            teta1: r("teta1")?,
            tmu: r("tmu")?,
            xtm0: r("xtm0")?,
            xtm1: r("xtm1")?,
            xtm2: r("xtm2")?,
            ks: r("ks")?,
            vsg: r("vsg")?,
            tf: values.get("tf").map(|v| v.value),
            flo: r("flo")?,
            delfo: r("delfo")?,
            ag: r("ag")?,
            tc1: r("rtc1")?,
            tc2: r("rtc2")?,
            zeta: r("zeta")?,
        };
        if model.vsigma == 0. {
            return Err(SpiceError::circuit(format!(
                "{}: MESA model '{}' needs VSIGMA > 0 (mesa1 divides by it)",
                resolved.card().location,
                resolved.card().name
            )));
        }
        if model.d == 0. || model.du + model.th == 0. {
            return Err(SpiceError::circuit(format!(
                "{}: MESA model '{}' needs a nonzero device depth",
                resolved.card().location,
                resolved.card().name
            )));
        }
        Ok(model)
    }
}

/// Validated instance setters.
#[derive(Debug, Clone, Copy)]
pub(super) struct Instance {
    pub(super) l: Real,
    pub(super) w: Real,
    pub(super) m: Real,
    /// Celsius as written (`mesaparam.c` adds 273.15); `None` is
    /// `CKTtemp + dtemp` used as kelvin.
    pub(super) td: Option<Real>,
    pub(super) ts: Option<Real>,
    pub(super) dtemp: Real,
}

impl Instance {
    pub(super) fn resolve(
        i: &crate::netlist::ast::DeviceInstance,
    ) -> SpiceResult<(
        crate::devices::initial::InstanceInitial,
        Vec<crate::netlist::ast::ParameterAssignment>,
    )> {
        crate::devices::initial::split(&i.parameters, &IC_COMPONENTS)
    }

    pub(super) fn validate(
        setters: &[crate::netlist::ast::ParameterAssignment],
        owner: &crate::primitives::SourceLoc,
    ) -> SpiceResult<Self> {
        let values = ScalarSchema {
            parameters: INSTANCE,
        }
        .validate(setters, owner)?;
        Ok(Self {
            l: jfet::required(&values, "l")?,
            w: jfet::required(&values, "w")?,
            m: jfet::required(&values, "m")?,
            td: values.get("td").map(|v| v.value),
            ts: values.get("ts").map(|v| v.value),
            dtemp: jfet::required(&values, "dtemp")?,
        })
    }
}
