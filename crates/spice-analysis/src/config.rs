//! Typed run configuration resolved from a deck's `.option` cards.
//!
//! C: `INPdoOpts()` (`inpdoopt.c`) applies settings left to right through
//! `CKTsetOpt()` (`cktsopt.c`). The port differs deliberately: nothing is
//! applied to global state, unknown or unimplemented options are errors instead
//! of warnings, and only settings an implemented engine honours take effect.
//!
//! # Supported options
//!
//! | Option | Effect |
//! | --- | --- |
//! | `temp`, `tnom` | [`AnalysisContext`] circuit/nominal temperature (Celsius, finite, above absolute zero) |
//! | `reltol` | transient relative tolerance (`rtol`): companion truncation/convergence, or diffsol BDF |
//! | `vntol` | transient voltage absolute tolerance (companion Newton test; diffsol BDF) |
//! | `abstol` | transient branch-current absolute tolerance (companion truncation/Newton test; diffsol BDF) |
//! | `chgtol`, `trtol` | companion local-truncation-error charge floor and overestimation factor; **rejected with `backend=diffsol`** |
//! | `method`, `maxord` | retained as [`RunConfig::method`]/[`RunConfig::maxord`] and forwarded to the companion driver (`trap`/`trapezoidal`/`gear`, `maxord` 1 or 2); **rejected with `backend=diffsol`**, which is neither |
//! | `itl1` | DC Newton iteration limit per stage (`maxiter`, 1..=10000) for `.op`/`.dc`/`.ac`; **rejected for `.tran`** |
//! | `srcsteps` | DC source-stepping increments (`0` disables, else 1..=1000 equal steps); **rejected for `.tran`** |
//! | `gminsteps`, `gminfactor` | DC gmin-stepping stage count (`0` disables, else 1..=100) and ratio (1 < factor <= 1e6, default 10) from 1e-3 S; **rejected for `.tran`** |
//!
//! `.ic` and `.nodeset` cards are not options: [`RunConfig::from_netlist`]
//! evaluates them against `.param` and [`RunConfig::request`] attaches them to
//! every analysis request (`AnalysisRequest::initial_conditions`/`nodesets`).
//!
//! The DC options are *not* C's `itl1`/`srcsteps`/`gminsteps` semantics verbatim
//! (this port's schedules are fixed and deterministic, and the default `itl1`
//! is 200, not 100); see `docs/port/DC_CONTINUATION.md`. `gmin` (the fixed
//! junction gmin) stays unimplemented: it is not an artificial continuation gmin.
//!
//! Every other name from `cktsopt.c` (`itl2`-`itl6`, `gmin`, flags,
//! ...) is reported as [`SpiceError::NotYetPorted`]; names absent from that
//! table are parse errors. `no_auto_gnd` is a front-end variable in C, not an
//! `.option`: use `Parser::with_auto_gnd`.
//!
//! # Precedence
//!
//! Highest first: explicit [`RunOverrides`] (temperatures) or explicit analysis
//! request arguments (`rtol=`, `vntol=`, `abstol=`, `chgtol=`, `trtol=`, `method=`,
//! `maxord=`, `maxsteps=`, and for DC/AC `maxiter=`, `srcsteps=`, `gminsteps=`,
//! `gminfactor=`), then the deck's options, then driver defaults (27 C). Each
//! name is resolved independently, so a request `gminsteps=` combines with a
//! deck `gminfactor`.
//! Defaults are the backend's: the companion driver uses ngspice's (reltol 1e-3,
//! vntol 1e-6, abstol 1e-12, chgtol 1e-14, trtol 7); diffsol BDF keeps rtol 1e-7,
//! vntol 1e-9, abstol 1e-12.
//! Repeated options override in deck order, last wins; giving one name both as a
//! flag and with a value is a conflict error. A [`RunConfig`] is a plain value
//! computed per deck, so nothing leaks between decks.

use spice_core::{AnalysisKind, Real, SourceLoc, SpiceError, SpiceResult, parse_spice_number};
use spice_devices::Circuit;
use spice_maths::integrator::IntegrationMethod;
use spice_netlist::ast::{AnalysisCard, Netlist, OptionCard, OptionSetting};

use crate::{AnalysisContext, AnalysisRequest, NodeCondition};

const C_REFERENCE: &str = "src/spicelib/analysis/cktsopt.c, src/spicelib/parser/inpdoopt.c";

/// Names in `cktsopt.c`'s `OPTtbl` that this port does not implement yet.
const KNOWN_UNIMPLEMENTED: &[&str] = &[
    "cshunt",
    "rshunt",
    "noopiter",
    "gmin",
    "gshunt",
    "pivtol",
    "pivrel",
    "itl2",
    "itl3",
    "itl4",
    "itl5",
    "itl6",
    "acct",
    "list",
    "nomod",
    "nopage",
    "node",
    "opts",
    "oldlimit",
    "numdgt",
    "cptime",
    "limtim",
    "limpts",
    "lvlcod",
    "lvltim",
    "indverbosity",
    "xmu",
    "defm",
    "defl",
    "defw",
    "minbreak",
    "defad",
    "defas",
    "bypass",
    "badmos3",
    "trytocompact",
    "keepopinfo",
    "copynodesets",
    "nodedamping",
    "linesearch",
    "absdv",
    "reldv",
    "noopac",
    "epsmin",
    "sparse",
    "klu",
    "lteabstol",
    "ltereltol",
    "ltetrtol",
    "newtrunc",
    "maxopalter",
    "maxevtiter",
    "noopalter",
    "ramptime",
    "convlimit",
    "convstep",
    "convabsstep",
    "autopartial",
];

/// Highest `maxord` ngspice accepts (`cktsopt.c` clamps to 1..=6).
const MAX_ORD: u8 = 6;

/// Settings that explicitly beat the deck's `.option` temperatures.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RunOverrides {
    /// Circuit temperature in Celsius, replacing a deck `temp`.
    pub temperature: Option<Real>,
    /// Nominal temperature in Celsius, replacing a deck `tnom`.
    pub nominal_temperature: Option<Real>,
}

/// Deck-supplied transient settings. `None` leaves the driver default, so the
/// BDF defaults are unchanged unless a deck sets them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TransientSettings {
    /// `reltol` → BDF relative tolerance.
    pub rtol: Option<Real>,
    /// `vntol` → voltage absolute tolerance.
    pub vntol: Option<Real>,
    /// `abstol` → branch-current absolute tolerance.
    pub abstol: Option<Real>,
    /// `chgtol` → companion charge/flux floor in truncation control.
    pub chgtol: Option<Real>,
    /// `trtol` → companion truncation-error overestimation factor.
    pub trtol: Option<Real>,
}

/// Option names that configure DC Newton/continuation, in no particular order.
const DC_OPTIONS: [&str; 4] = ["itl1", "srcsteps", "gminsteps", "gminfactor"];

/// Deck-supplied DC settings (last occurrence wins). `None` leaves the default;
/// `Some(0)` disables source/gmin stepping.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DcOptions {
    /// `itl1` → Newton iteration limit per DC stage.
    pub itl1: Option<usize>,
    /// `srcsteps` → equal source-stepping increments.
    pub srcsteps: Option<usize>,
    /// `gminsteps` → gmin-stepping stages.
    pub gminsteps: Option<usize>,
    /// `gminfactor` → ratio between consecutive gmin stages.
    pub gminfactor: Option<Real>,
}

impl DcOptions {
    /// The continuation policy these options resolve to over the defaults.
    ///
    /// # Errors
    /// An invalid combination, e.g. a factor whose schedule underflows.
    pub fn policy(&self) -> SpiceResult<crate::bias::ContinuationPolicy> {
        crate::bias::ContinuationPolicy::from_steps(self.srcsteps, self.gminsteps, self.gminfactor)
    }
}

/// One accepted option occurrence, kept in deck order for diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedOption {
    /// Lowercased option name.
    pub name: String,
    /// Value text as written.
    pub value: String,
    /// Where the option name was written.
    pub location: SourceLoc,
}

/// Resolved, validated run configuration for one deck.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunConfig {
    context: AnalysisContext,
    transient: TransientSettings,
    dc: DcOptions,
    method: Option<(String, SourceLoc)>,
    maxord: Option<(u8, SourceLoc)>,
    applied: Vec<AppliedOption>,
    /// Evaluated `.ic` entries in deck order, attached to every request.
    initial_conditions: Vec<NodeCondition>,
    /// Evaluated `.nodeset` entries in deck order, attached to every request.
    nodesets: Vec<NodeCondition>,
    params: Option<std::sync::Arc<spice_netlist::eval::ParamScope>>,
}

impl RunConfig {
    /// Resolve all `.option` cards of a deck with no explicit overrides.
    ///
    /// # Errors
    /// Unknown, unimplemented, malformed, out-of-range or conflicting options.
    pub fn from_netlist(netlist: &Netlist) -> SpiceResult<Self> {
        let mut config = Self::from_options(&netlist.options, &RunOverrides::default())?;
        let scope = spice_netlist::eval::ParamScope::for_netlist(netlist)?;
        let (initial, nodesets) = spice_netlist::elaborate::literalize_node_hints(
            netlist,
            &scope,
            &mut spice_netlist::eval::EvalBudget::default(),
        )?;
        let conditions = |cards: &[spice_netlist::ast::NodeHintCard]| {
            cards
                .iter()
                .flat_map(|card| &card.entries)
                .filter_map(|entry| {
                    Some(NodeCondition {
                        node: entry.node.to_string(),
                        value: entry.literal()?,
                        location: entry.location.clone(),
                    })
                })
                .collect::<Vec<_>>()
        };
        config.initial_conditions = conditions(&initial);
        config.nodesets = conditions(&nodesets);
        config.params = Some(std::sync::Arc::new(scope));
        Ok(config)
    }

    /// The deck's resolved top-level `.param` scope (only for configs built by
    /// [`Self::from_netlist`]).
    #[must_use]
    pub fn params(&self) -> Option<&spice_netlist::eval::ParamScope> {
        self.params.as_deref()
    }

    /// Resolve option cards in order, then apply `overrides`.
    ///
    /// # Errors
    /// As [`Self::from_netlist`]; overrides must also be valid temperatures.
    pub fn from_options(cards: &[OptionCard], overrides: &RunOverrides) -> SpiceResult<Self> {
        let mut config = Self::default();
        let mut flagged = std::collections::BTreeSet::new();
        let mut valued = std::collections::BTreeSet::new();
        for setting in cards.iter().flat_map(|card| &card.settings) {
            let seen = if setting.value.is_some() {
                &mut valued
            } else {
                &mut flagged
            };
            seen.insert(setting.name.as_str());
            if flagged.contains(setting.name.as_str()) && valued.contains(setting.name.as_str()) {
                return Err(SpiceError::parse(
                    setting.location.clone(),
                    format!(
                        "option '{}' is given both as a flag and with a value",
                        setting.name
                    ),
                ));
            }
            config.apply(setting)?;
        }
        // The last-set counts and factor must form one valid schedule together.
        if let Err(error) = config.dc.policy() {
            let location = config
                .applied
                .iter()
                .rev()
                .find(|option| DC_OPTIONS.contains(&option.name.as_str()))
                .map(|option| option.location.clone())
                .unwrap_or_else(|| SourceLoc::new(std::path::PathBuf::from("<options>"), 0, 0));
            return Err(SpiceError::parse(location, error.to_string()));
        }
        let unknown = SourceLoc::new(std::path::PathBuf::from("<run overrides>"), 0, 0);
        if let Some(t) = overrides.temperature {
            config.context.temperature = temperature(t, &unknown, "temperature override")?;
        }
        if let Some(t) = overrides.nominal_temperature {
            config.context.nominal_temperature =
                temperature(t, &unknown, "nominal temperature override")?;
        }
        Ok(config)
    }

    fn apply(&mut self, setting: &OptionSetting) -> SpiceResult<()> {
        let name = setting.name.as_str();
        let supported = matches!(
            name,
            "temp"
                | "tnom"
                | "reltol"
                | "vntol"
                | "abstol"
                | "chgtol"
                | "trtol"
                | "method"
                | "maxord"
                | "itl1"
                | "srcsteps"
                | "gminsteps"
                | "gminfactor"
        );
        if !supported {
            if KNOWN_UNIMPLEMENTED.contains(&name) {
                return Err(SpiceError::not_yet_ported(
                    format!("{}: option '{name}'", setting.location),
                    C_REFERENCE,
                ));
            }
            return Err(SpiceError::parse(
                setting.location.clone(),
                if name == "no_auto_gnd" {
                    "unknown option 'no_auto_gnd' (a front-end variable; use Parser::with_auto_gnd \
                     or --no-auto-gnd)"
                        .to_owned()
                } else {
                    format!("unknown option '{name}'")
                },
            ));
        }
        let Some(value) = &setting.value else {
            return Err(SpiceError::parse(
                setting.location.clone(),
                format!("option '{name}' requires a value"),
            ));
        };
        let location = &setting.location;
        match name {
            "method" => {
                if IntegrationMethod::parse(&value.text, 2).is_none() {
                    return Err(SpiceError::parse(
                        value.location.clone(),
                        format!(
                            "method must be trap, trapezoidal or gear, not '{}'",
                            value.text
                        ),
                    ));
                }
                self.method = Some((value.text.to_ascii_lowercase(), location.clone()));
            }
            "maxord" => {
                let order = integer(&value.text)
                    .and_then(|n| u8::try_from(n).ok())
                    .filter(|n| (1..=MAX_ORD).contains(n))
                    .ok_or_else(|| {
                        SpiceError::parse(
                            value.location.clone(),
                            format!(
                                "maxord must be an integer in 1..={MAX_ORD}, not '{}'",
                                value.text
                            ),
                        )
                    })?;
                self.maxord = Some((order, location.clone()));
            }
            "itl1" | "srcsteps" | "gminsteps" => {
                let (lowest, highest) = match name {
                    "itl1" => (1, crate::newton::MAX_ITERATIONS),
                    "srcsteps" => (0, crate::bias::MAX_SOURCE_STEPS),
                    _ => (0, crate::bias::MAX_GMIN_STAGES),
                };
                let count = integer(&value.text)
                    .map(|n| n as usize)
                    .filter(|n| (lowest..=highest).contains(n))
                    .ok_or_else(|| {
                        SpiceError::parse(
                            value.location.clone(),
                            format!(
                                "option '{name}' must be an integer in {lowest}..={highest}, \
                                 not '{}'",
                                value.text
                            ),
                        )
                    })?;
                match name {
                    "itl1" => self.dc.itl1 = Some(count),
                    "srcsteps" => self.dc.srcsteps = Some(count),
                    _ => self.dc.gminsteps = Some(count),
                }
            }
            "gminfactor" => {
                let factor = parse_spice_number(&value.text)
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| {
                        SpiceError::parse(
                            value.location.clone(),
                            format!(
                                "option '{name}' needs a finite number, not '{}'",
                                value.text
                            ),
                        )
                    })?;
                self.dc.gminfactor = Some(factor);
            }
            _ => {
                let number = parse_spice_number(&value.text)
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| {
                        SpiceError::parse(
                            value.location.clone(),
                            format!(
                                "option '{name}' needs a finite number, not '{}'",
                                value.text
                            ),
                        )
                    })?;
                match name {
                    "temp" => self.context.temperature = temperature(number, location, name)?,
                    "tnom" => {
                        self.context.nominal_temperature = temperature(number, location, name)?;
                    }
                    _ => {
                        if number <= 0. {
                            return Err(SpiceError::parse(
                                location.clone(),
                                format!("option '{name}' must be positive"),
                            ));
                        }
                        match name {
                            "reltol" => self.transient.rtol = Some(number),
                            "vntol" => self.transient.vntol = Some(number),
                            "abstol" => self.transient.abstol = Some(number),
                            "chgtol" => self.transient.chgtol = Some(number),
                            _ => self.transient.trtol = Some(number),
                        }
                    }
                }
            }
        }
        self.applied.push(AppliedOption {
            name: setting.name.clone(),
            value: value.text.clone(),
            location: location.clone(),
        });
        Ok(())
    }

    /// Temperatures shared by all analyses; defaults are 27 C / 27 C.
    #[must_use]
    pub const fn context(&self) -> AnalysisContext {
        self.context
    }

    /// Deck-supplied transient tolerances (unset fields keep driver defaults).
    #[must_use]
    pub const fn transient(&self) -> &TransientSettings {
        &self.transient
    }

    /// Deck-supplied DC Newton/continuation settings (unset fields keep the
    /// defaults of [`crate::bias::DcSettings`]).
    #[must_use]
    pub const fn dc(&self) -> &DcOptions {
        &self.dc
    }

    /// Retained `method=` selection, lowercased, if the deck gave one.
    #[must_use]
    pub fn method(&self) -> Option<&str> {
        self.method.as_ref().map(|(name, _)| name.as_str())
    }

    /// Retained `maxord=` selection, if the deck gave one.
    #[must_use]
    pub fn maxord(&self) -> Option<u8> {
        self.maxord.as_ref().map(|(order, _)| *order)
    }

    /// Accepted options in deck order (duplicates included).
    #[must_use]
    pub fn applied(&self) -> &[AppliedOption] {
        &self.applied
    }

    /// Elaborate `netlist` at this configuration's temperatures.
    ///
    /// # Errors
    /// As [`Circuit::from_netlist_with_context`].
    pub fn circuit(&self, netlist: &Netlist) -> SpiceResult<Circuit> {
        Circuit::from_netlist_with_context(netlist, &self.context.model_context())
    }

    /// Build the request for an analysis card, evaluating braced `{expr}`
    /// arguments against the deck's `.param` scope and filling deck-level
    /// settings the card did not state itself.
    ///
    /// # Errors
    /// As [`Self::request`].
    pub fn request_for(&self, card: &AnalysisCard) -> SpiceResult<AnalysisRequest> {
        if card.expressions.is_empty() {
            return self.request(AnalysisRequest::from(card));
        }
        let Some(scope) = &self.params else {
            return Err(SpiceError::Unsupported {
                feature: "braced analysis arguments need a RunConfig built with \
                          RunConfig::from_netlist (no parameter scope here)"
                    .into(),
                location: Some(card.location.clone()),
            });
        };
        let literal = spice_netlist::elaborate::literalize_analysis(
            card,
            scope,
            &mut spice_netlist::eval::EvalBudget::default(),
        )?;
        self.request(AnalysisRequest::from(&literal))
    }

    /// Add deck settings to a request. Explicit request arguments win; the
    /// integration options apply only to `.tran`; physical tolerances also reach
    /// the M4 DC/AC Newton solve, as do `itl1` (as `maxiter`), `srcsteps`,
    /// `gminsteps` and `gminfactor`, which only DC/AC consume.
    ///
    /// # Errors
    /// [`SpiceError::Unsupported`] when the deck selected `method`/`maxord`
    /// (or `chgtol`/`trtol`) and the transient request names `backend=diffsol`,
    /// which implements neither trap/Gear nor truncation control, so the
    /// selection cannot be honoured and is not silently ignored; also Gear
    /// orders above 2, and any deck DC option (`itl1`, `srcsteps`, `gminsteps`,
    /// `gminfactor`) for a transient request, whose initial bias does not read
    /// them.
    pub fn request(&self, mut request: AnalysisRequest) -> SpiceResult<AnalysisRequest> {
        // Deck-level hints travel with every request (explicit request entries
        // win); each driver validates them against its circuit.
        if request.initial_conditions.is_empty() {
            request
                .initial_conditions
                .clone_from(&self.initial_conditions);
        }
        if request.nodesets.is_empty() {
            request.nodesets.clone_from(&self.nodesets);
        }
        if request.kind != AnalysisKind::Transient {
            if matches!(
                request.kind,
                AnalysisKind::OperatingPoint | AnalysisKind::DcSweep | AnalysisKind::Ac
            ) {
                for (key, value) in [
                    ("rtol", self.transient.rtol),
                    ("vntol", self.transient.vntol),
                    ("abstol", self.transient.abstol),
                ] {
                    if let (Some(value), None) = (value, request.named(key)) {
                        request.arguments.push(format!("{key}={value:e}"));
                    }
                }
                for (key, value) in [
                    ("maxiter", self.dc.itl1),
                    ("srcsteps", self.dc.srcsteps),
                    ("gminsteps", self.dc.gminsteps),
                ] {
                    if let (Some(value), None) = (value, request.named(key)) {
                        request.arguments.push(format!("{key}={value}"));
                    }
                }
                if let (Some(factor), None) = (self.dc.gminfactor, request.named("gminfactor")) {
                    request.arguments.push(format!("gminfactor={factor:e}"));
                }
            }
            return Ok(request);
        }
        if let Some(option) = self
            .applied
            .iter()
            .find(|option| DC_OPTIONS.contains(&option.name.as_str()))
        {
            return Err(SpiceError::Unsupported {
                feature: format!(
                    ".option {}: DC Newton/continuation settings apply to .op/.dc/.ac; the \
                     transient initial bias does not read them (omit the option, or run the \
                     deck's DC analyses separately)",
                    option.name
                ),
                location: Some(option.location.clone()),
            });
        }
        let diffsol = request
            .named("backend")
            .is_some_and(|name| name.eq_ignore_ascii_case("diffsol"));
        if let Some((name, location)) = &self.method {
            let order = self.maxord.as_ref().map_or(2, |(order, _)| *order);
            let method = IntegrationMethod::parse(name, order).expect("validated at parse");
            method.validate_runtime()?;
            if diffsol {
                return Err(unsupported_integration(&format!("method={name}"), location));
            }
            if request.named("method").is_none() {
                request.arguments.push(format!("method={name}"));
            }
        }
        if let Some((order, location)) = &self.maxord {
            if diffsol {
                return Err(unsupported_integration(
                    &format!("maxord={order}"),
                    location,
                ));
            }
            if request.named("maxord").is_none() {
                request.arguments.push(format!("maxord={order}"));
            }
        }
        for (key, value) in [
            ("chgtol", self.transient.chgtol),
            ("trtol", self.transient.trtol),
        ] {
            if value.is_some() && diffsol {
                return Err(SpiceError::Unsupported {
                    feature: format!(
                        ".option {key}: local-truncation control belongs to the companion \
                         trap/Gear driver, not backend=diffsol"
                    ),
                    location: None,
                });
            }
        }
        for (key, value) in [
            ("rtol", self.transient.rtol),
            ("vntol", self.transient.vntol),
            ("abstol", self.transient.abstol),
            ("chgtol", self.transient.chgtol),
            ("trtol", self.transient.trtol),
        ] {
            if let (Some(value), None) = (value, request.named(key)) {
                request.arguments.push(format!("{key}={value:e}"));
            }
        }
        Ok(request)
    }
}

fn unsupported_integration(what: &str, location: &SourceLoc) -> SpiceError {
    SpiceError::Unsupported {
        feature: format!(
            ".option {what}: backend=diffsol is adaptive BDF, not ngspice trap/Gear; \
             omit backend= to run the companion driver"
        ),
        location: Some(location.clone()),
    }
}

fn integer(text: &str) -> Option<u32> {
    let value = parse_spice_number(text).filter(|v| v.is_finite() && v.fract() == 0.)?;
    (0. ..=f64::from(u32::MAX))
        .contains(&value)
        .then_some(value as u32)
}

fn temperature(value: Real, location: &SourceLoc, what: &str) -> SpiceResult<Real> {
    if value.is_finite() && value > -273.15 {
        Ok(value)
    } else {
        Err(SpiceError::parse(
            location.clone(),
            format!("{what} must be finite and above absolute zero (-273.15 C), got {value}"),
        ))
    }
}
