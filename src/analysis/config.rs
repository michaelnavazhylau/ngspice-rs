//! Typed run configuration resolved from a deck's `.option` cards.
//!
//! C: `INPdoOpts()` (`inpdoopt.c`) applies settings left to right through
//! `CKTsetOpt()` (`cktsopt.c`); the front end first handles its own variables
//! (`spiceif.c::if_option`, `options.c`). The port differs deliberately: nothing
//! is applied to global state, unknown or unimplemented options are errors
//! instead of warnings, and every accepted name either takes effect or is a
//! documented no-op ([`RunConfig::ignored`]) justified by C behaviour.
//!
//! # Options with an effect
//!
//! | Option | Effect |
//! | --- | --- |
//! | `temp`, `tnom` | [`AnalysisContext`] circuit/nominal temperature (Celsius, finite, above absolute zero) |
//! | `gmin` | [`AnalysisContext::gmin`]: junction minimum conductance of every diode/BJT/MOS1 junction in all analyses (C `CKTgmin`, default 1e-12 S; finite, `>= 0`) |
//! | `reltol` | transient relative tolerance (`rtol`): companion truncation/convergence, or diffsol BDF; also the DC/AC Newton `rtol` |
//! | `vntol` | voltage absolute tolerance (companion Newton test; diffsol BDF; DC/AC Newton) |
//! | `abstol` | branch-current absolute tolerance (companion truncation/Newton test; diffsol BDF; DC/AC Newton) |
//! | `chgtol`, `trtol` | companion local-truncation-error charge floor and overestimation factor; **rejected with `backend=diffsol`** |
//! | `method`, `maxord` | retained as [`RunConfig::method`]/[`RunConfig::maxord`] and forwarded to the companion driver (`trap`/`trapezoidal`/`gear`, `maxord` 1 to 6; `dctran.c` never runs above order 2, so 2 to 6 behave alike); **rejected with `backend=diffsol`**, which is neither |
//! | `xmu` | companion trapezoidal weighting (`nicomcof.c`, default 0.5, `0..=0.5`); **rejected with `backend=diffsol`** |
//! | `itl1` | Newton iteration limit of the direct DC solve and of the gmin strategies' closing solve (`maxiter`) for `.op`/`.dc`/`.ac` and the companion `.tran` initial bias (C `CKTdcMaxIter`; `dcop.c`, `acan.c`, `dctran.c` call `CKTop`); default 100 |
//! | `itl2` | Newton limit of every other gmin/source-stepping stage of that DC bias (`stagemaxiter`, C `CKTdcTrcvMaxIter` in `cktop.c`); as written, the step adaptation of `dynamic_gmin`/`new_gmin`/`gillespie_src` (`adaptiter`, `iters <= itl2/4`); on `.dc` also the warm-started solve at every sweep point after the first (`trcvmaxiter`, `dctrcurv.c`), whose failure falls back to the full bias |
//! | `itl4` | companion `.tran` Newton iterations per timepoint (`tranmaxiter`; effective default 100 as in C) |
//! | `srcsteps` (alias `itl6`) | C's source stepping: `1` (default) `gillespie_src`, `n > 1` `spice3_src` with `n` equal increments, `0` disables (`0..=1000`) |
//! | `gminsteps`, `gminfactor` | C's gmin stepping: `1` (default) `dynamic_gmin` then `new_gmin`, `n > 1` `spice3_gmin` from `gmin * gminfactor^n`, `0` disables (`0..=100`); ratio `1 < factor <= 1e6`, default 10 |
//! | `noopiter` (flag) | skip the direct Newton attempt of every DC bias (C `CKTnoOpIter`) |
//!
//! `itl1`/`itl2`/`itl4` take integers in `0..=10000` and are stored as C's
//! *effective* limit `max(n, 100)`: `NIiter()` (`niiter.c`) raises every limit
//! below 100 to 100, so smaller values change nothing in C or here (C's
//! nominal `itl4` default of 10 and `itl2` default of 50 are effectively 100
//! too); the raw `itl2` is kept separately for the step adaptation. When a deck
//! sets `itl1` or `itl2`, the continuation stages use `itl2` (default 100) as in
//! C; without either, every stage uses `itl1`'s default 100, which is the same
//! number. Unlike C's `IF_INTEGER` options, a non-integer (`itl4=2.5`) is
//! rejected rather than rounded.
//!
//! The DC options reach `.op`, `.dc`, `.ac` and the companion `.tran` initial
//! bias; with `backend=diffsol` they (and `itl4`, `xmu`) are rejected. They
//! follow C's `CKTop` strategies (#106); the port-only request keys
//! `continuation=ladder` and `limiting=global` select the port's earlier fixed
//! ladders and global damping instead. C's `OPtran` fallback, `gshunt`,
//! `oldlimit`, predictor and bypass are not ported; see
//! `docs/port/DC_CONTINUATION.md`.
//!
//! # Documented no-ops
//!
//! Accepted, validated, recorded in [`RunConfig::ignored`] with the reason, and
//! without numerical effect in C or in this port:
//!
//! * flags `acct`, `noacct`, `list`, `nomod`, `nopage`, `node`, `opts`,
//!   `noinit`, `norefvalue`: front-end print controls (`spiceif.c::if_option`);
//! * `itl3`, `itl5`, `cptime`, `limtim`, `limpts`, `lvlcod`, `lvltim`: C ignores
//!   them (`OPTtbl` entries without `IF_SET`; `if_option` warns
//!   "unsupported"/"obsolete");
//! * `post`, `ingold` (flag or value): plain front-end variables nothing in
//!   ngspice reads;
//! * `indverbosity=N` (a non-negative integer): only selects which stderr
//!   diagnostics C's inductive-system check prints (`muttemp.c`); the port
//!   prints none and always rejects a coupled inductance matrix that is not
//!   positive semidefinite (`docs/port/MUTUAL_INDUCTANCE.md`);
//! * `bypass=0`: C's default (`cktntask.c`); this port never bypasses device
//!   evaluation. Any other `bypass` is `NotYetPorted`.
//!
//! Every other name from `cktsopt.c` (`gshunt`, `oldlimit`, `minbreak`, ...)
//! and the front-end variables with an effect (`filetype`, `numdgt`,
//! `savecurrents`, `scale`, `seed`, ...) is [`SpiceError::NotYetPorted`]; names
//! absent from both are parse errors. `no_auto_gnd` is a front-end variable in
//! C, not an `.option`: use `Parser::with_auto_gnd`.
//!
//! # Expression values
//!
//! `{expr}` and `'expr'` values (C: numparam substitutes both) are evaluated by
//! [`RunConfig::from_netlist`] against the deck's top-level `.param` scope;
//! [`RunConfig::from_options`] has no scope and rejects them. `method` takes a
//! word, never an expression.
//!
//! `.ic` and `.nodeset` cards are not options: [`RunConfig::from_netlist`]
//! evaluates them against `.param` and [`RunConfig::request`] attaches them to
//! every analysis request (`AnalysisRequest::initial_conditions`/`nodesets`).
//!
//! # Precedence
//!
//! Highest first: explicit [`RunOverrides`] (temperatures) or explicit analysis
//! request arguments (`rtol=`, `vntol=`, `abstol=`, `chgtol=`, `trtol=`, `method=`,
//! `maxord=`, `xmu=`, `tranmaxiter=`, `maxsteps=`, `maxiter=`, `srcsteps=`,
//! `gminsteps=`, `gminfactor=`, `noopiter=`, `adaptiter=`, `trcvmaxiter=`),
//! then the deck's options, then driver defaults (27 C). Each name is resolved independently, so a request
//! `gminsteps=` combines with a deck `gminfactor`.
//! Defaults are the backend's: the companion driver uses ngspice's (reltol 1e-3,
//! vntol 1e-6, abstol 1e-12, chgtol 1e-14, trtol 7); diffsol BDF keeps rtol 1e-7,
//! vntol 1e-9, abstol 1e-12.
//! Repeated options override in deck order, last wins (`itl6` and `srcsteps`
//! are one setting); giving one name both as a flag and with a value is a
//! conflict error. A [`RunConfig`] is a plain value computed per deck, so
//! nothing leaks between decks.

use crate::devices::Circuit;
use crate::maths::integrator::IntegrationMethod;
use crate::netlist::ast::{AnalysisCard, Netlist, OptionCard, OptionSetting};
use crate::netlist::eval::{EvalBudget, ParamScope};
use crate::primitives::{
    AnalysisKind, Real, SourceLoc, SpiceError, SpiceResult, parse_spice_number,
};

use crate::analysis::{AnalysisContext, AnalysisRequest, NodeCondition};

const C_REFERENCE: &str = "src/spicelib/analysis/cktsopt.c, src/spicelib/parser/inpdoopt.c";
const FRONTEND_REFERENCE: &str =
    "src/frontend/options.c, src/frontend/spiceif.c, src/frontend/inp.c";

/// Names in `cktsopt.c`'s `OPTtbl` that this port does not implement yet.
const KNOWN_UNIMPLEMENTED: &[&str] = &[
    "cshunt",
    "rshunt",
    "gshunt",
    "oldlimit",
    "numdgt",
    "defm",
    "defl",
    "defw",
    "minbreak",
    "defad",
    "defas",
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
    "klu_memgrow_factor",
    "lteabstol",
    "ltereltol",
    "ltetrtol",
    "newtrunc",
    "maxopalter",
    // Sparse 1.3 pivot thresholds (TSKpivotAbsTol/TSKpivotRelTol); this port's
    // faer partial-pivoting LU has no equivalent knob yet.
    "pivtol",
    "pivrel",
    "maxevtiter",
    "noopalter",
    "ramptime",
    "convlimit",
    "convstep",
    "convabsstep",
    "autopartial",
];

/// Front-end variables that ngspice reads from `.options` with an effect on
/// output or setup, not implemented here.
const FRONTEND_UNIMPLEMENTED: &[&str] = &[
    "filetype",
    "savecurrents",
    "scale",
    "scalm",
    "seed",
    "seedinfo",
    "rndseed",
    "interp",
    "warn",
    "measureprec",
    "rawfileprec",
    "strict_errorhandling",
];

/// Front-end print-control flags: no numerical effect in C (`spiceif.c`).
const PRINT_FLAGS: &[&str] = &[
    "acct",
    "noacct",
    "list",
    "nomod",
    "nopage",
    "node",
    "opts",
    "noinit",
    "norefvalue",
];
const PRINT_FLAG_REASON: &str = "front-end print control (spiceif.c if_option); no numerical \
                                 effect, and this port prints no such listing or accounting";

/// `OPTtbl` entries C parses but never applies (no `IF_SET`), with whether
/// the value must be an integer.
const IGNORED_BY_C: &[(&str, bool)] = &[
    ("itl3", true),
    ("itl5", true),
    ("cptime", false),
    ("limtim", true),
    ("limpts", true),
    ("lvlcod", true),
    ("lvltim", true),
];
const IGNORED_BY_C_REASON: &str = "ignored by ngspice (OPTtbl entry without IF_SET; \
                                   spiceif.c reports it unsupported or obsolete)";
const UNREAD_REASON: &str = "plain ngspice front-end variable that nothing reads";
const INDVERBOSITY_REASON: &str = "controls only the stderr diagnostics of C's inductive-system \
                                   check (muttemp.c, CKTindverbosity); this port never prints \
                                   them and always rejects a coupled inductance matrix that is \
                                   not positive semidefinite";
const BYPASS_REASON: &str = "bypass=0 is ngspice's default (cktntask.c); this port never \
                             bypasses device evaluation";

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
    /// `itl4` → companion Newton iterations per timepoint (`tranmaxiter`),
    /// stored as C's effective `max(itl4, 100)` (see [`NIITER_MIN_ITERATIONS`]).
    pub itl4: Option<usize>,
    /// `xmu` → companion trapezoidal weighting.
    pub xmu: Option<Real>,
}

/// `NIiter()` (`niiter.c`) raises every Newton iteration limit below this to
/// it; deck `itl1`/`itl2`/`itl4` values are stored as C's effective limit.
pub const NIITER_MIN_ITERATIONS: usize = 100;

/// Option names that configure the DC Newton/continuation solve, in no
/// particular order (`itl6` is stored as `srcsteps`).
const DC_OPTIONS: [&str; 7] = [
    "itl1",
    "itl2",
    "srcsteps",
    "itl6",
    "gminsteps",
    "gminfactor",
    "noopiter",
];

/// C's default `itl2` (`cktntask.c`: `TSKdcTrcvMaxIter = 50`) as `NIiter()`
/// applies it: raised to [`NIITER_MIN_ITERATIONS`].
pub const C_DEFAULT_ITL2_EFFECTIVE: usize = NIITER_MIN_ITERATIONS;

/// Options only the companion transient driver implements.
const COMPANION_ONLY: [&str; 2] = ["itl4", "xmu"];

/// Deck-supplied DC settings (last occurrence wins). `None` leaves the default;
/// `Some(0)` disables source/gmin stepping.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DcOptions {
    /// `itl1` → Newton iteration limit of the direct DC solve (C
    /// `CKTdcMaxIter`), stored as C's effective `max(itl1, 100)` (see
    /// [`NIITER_MIN_ITERATIONS`]).
    pub itl1: Option<usize>,
    /// `itl2` → Newton limit of every gmin/source-stepping stage (C
    /// `CKTdcTrcvMaxIter` in `cktop.c`) and of the `.dc` warm start at points
    /// after the first (`dctrcurv.c`), stored as C's effective `max(itl2, 100)`.
    pub itl2: Option<usize>,
    /// `itl2` as written: the adaptation base of the ngspice continuation
    /// strategies (`iters <= itl2 / 4`, [`crate::analysis::bias::NgspiceStepping`]).
    pub itl2_written: Option<usize>,
    /// `noopiter` → skip the direct Newton attempt of every DC bias (C
    /// `CKTnoOpIter`).
    pub noopiter: bool,
    /// `srcsteps`/`itl6` → equal source-stepping increments.
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
    pub fn policy(&self) -> SpiceResult<crate::analysis::bias::ContinuationPolicy> {
        let mut policy = crate::analysis::bias::ContinuationPolicy::from_ngspice_steps(
            self.srcsteps,
            self.gminsteps,
            self.gminfactor,
            self.itl2_written,
        )?;
        policy.skip_direct = self.noopiter;
        Ok(policy)
    }
}

/// One accepted option occurrence, kept in deck order for diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedOption {
    /// Lowercased option name.
    pub name: String,
    /// Value text as written (an expression keeps its `{...}`/`'...'` text).
    pub value: String,
    /// Where the option name was written.
    pub location: SourceLoc,
}

/// An accepted option occurrence that has no effect, with the C-based reason.
#[derive(Debug, Clone, PartialEq)]
pub struct IgnoredOption {
    /// Lowercased option name.
    pub name: String,
    /// Value text as written; `None` for a flag.
    pub value: Option<String>,
    /// Why it has no effect (C behaviour it mirrors).
    pub reason: &'static str,
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
    ignored: Vec<IgnoredOption>,
    /// Evaluated `.ic` entries in deck order, attached to every request.
    initial_conditions: Vec<NodeCondition>,
    /// Evaluated `.nodeset` entries in deck order, attached to every request.
    nodesets: Vec<NodeCondition>,
    params: Option<std::sync::Arc<ParamScope>>,
}

impl RunConfig {
    /// Resolve all `.option` cards of a deck with no explicit overrides,
    /// evaluating `{expr}`/`'expr'` option values against top-level `.param`.
    ///
    /// # Errors
    /// Unknown, unimplemented, malformed, out-of-range or conflicting options,
    /// and option expressions that fail to evaluate.
    pub fn from_netlist(netlist: &Netlist) -> SpiceResult<Self> {
        let scope = ParamScope::for_netlist(netlist)?;
        let mut config = Self::resolve(&netlist.options, &RunOverrides::default(), Some(&scope))?;
        let (initial, nodesets) = crate::netlist::elaborate::literalize_node_hints(
            netlist,
            &scope,
            &mut EvalBudget::default(),
        )?;
        let conditions = |cards: &[crate::netlist::ast::NodeHintCard]| {
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
    pub fn params(&self) -> Option<&ParamScope> {
        self.params.as_deref()
    }

    /// Resolve option cards in order, then apply `overrides`.
    ///
    /// # Errors
    /// As [`Self::from_netlist`]; overrides must also be valid temperatures.
    /// Expression values are [`SpiceError::Unsupported`] here (no `.param`
    /// scope): use [`Self::from_netlist`].
    pub fn from_options(cards: &[OptionCard], overrides: &RunOverrides) -> SpiceResult<Self> {
        Self::resolve(cards, overrides, None)
    }

    fn resolve(
        cards: &[OptionCard],
        overrides: &RunOverrides,
        scope: Option<&ParamScope>,
    ) -> SpiceResult<Self> {
        let mut config = Self::default();
        let mut flagged = std::collections::BTreeSet::new();
        let mut valued = std::collections::BTreeSet::new();
        let mut budget = EvalBudget::default();
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
            let evaluated = match (&setting.expression, scope) {
                (None, _) => None,
                // Unknown/unported names fail as such in `apply`, before any
                // evaluation error could mask them.
                (Some(_), _) if !accepted(&setting.name) => None,
                (Some(_), _) if setting.name == "method" => {
                    return Err(SpiceError::parse(
                        setting.location.clone(),
                        "option 'method' takes a word (trap, trapezoidal or gear), not an \
                         expression",
                    ));
                }
                (Some(expression), Some(scope)) => Some(
                    scope
                        .evaluate(expression, &mut budget)
                        .map_err(|error| match error {
                            SpiceError::Parse { location, message } => SpiceError::parse(
                                location,
                                format!(
                                    "{message}\n  while evaluating option '{}' at {}",
                                    setting.name, setting.location
                                ),
                            ),
                            other => other,
                        })?,
                ),
                (Some(_), None) => {
                    return Err(SpiceError::Unsupported {
                        feature: format!(
                            "expression value of option '{}' needs the deck's .param scope \
                             (use RunConfig::from_netlist)",
                            setting.name
                        ),
                        location: Some(setting.location.clone()),
                    });
                }
            };
            config.apply(setting, evaluated)?;
        }
        // The last-set counts and factor must form one valid schedule together
        // (with the deck's junction gmin, which seeds C's `spice3_gmin` ladder).
        if let Err(error) = config.dc.policy().and_then(|policy| match policy.schedule {
            crate::analysis::bias::ContinuationSchedule::Ngspice(stepping)
                if stepping.gmin_steps > 1 =>
            {
                stepping.spice3_ladder(config.context.gmin).map(|_| ())
            }
            _ => Ok(()),
        }) {
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

    /// Record a validated no-op occurrence.
    fn ignore(&mut self, setting: &OptionSetting, reason: &'static str) {
        self.ignored.push(IgnoredOption {
            name: setting.name.clone(),
            value: setting.value.as_ref().map(|value| value.text.clone()),
            reason,
            location: setting.location.clone(),
        });
    }

    /// Apply one setting; `evaluated` is the value of an expression setting.
    fn apply(&mut self, setting: &OptionSetting, evaluated: Option<Real>) -> SpiceResult<()> {
        let name = setting.name.as_str();
        let location = &setting.location;
        // The value as a number: an evaluated expression, or the literal text.
        let value_location = setting
            .value
            .as_ref()
            .map_or(location, |value| &value.location);
        let number = |what: &str| -> SpiceResult<Real> {
            let parsed = match (evaluated, &setting.value) {
                (Some(value), _) => Some(value),
                (None, Some(value)) => parse_spice_number(&value.text),
                (None, None) => None,
            };
            parsed.filter(|v| v.is_finite()).ok_or_else(|| {
                SpiceError::parse(
                    value_location.clone(),
                    format!(
                        "option '{name}' needs {what}, not '{}'",
                        setting.value.as_ref().map_or("", |v| v.text.as_str())
                    ),
                )
            })
        };
        let whole = |lowest: u32, highest: u32| -> SpiceResult<usize> {
            let value = number("an integer")?;
            integer_value(value)
                .filter(|n| (lowest..=highest).contains(n))
                .map(|n| n as usize)
                .ok_or_else(|| {
                    SpiceError::parse(
                        value_location.clone(),
                        format!("option '{name}' must be an integer in {lowest}..={highest}, not {value}"),
                    )
                })
        };
        if PRINT_FLAGS.contains(&name) {
            if setting.value.is_some() {
                return Err(SpiceError::parse(
                    location.clone(),
                    format!("option '{name}' is a flag and takes no value"),
                ));
            }
            self.ignore(setting, PRINT_FLAG_REASON);
            return Ok(());
        }
        if let Some((_, integral)) = IGNORED_BY_C.iter().find(|(n, _)| *n == name) {
            if setting.value.is_none() {
                return Err(SpiceError::parse(
                    location.clone(),
                    format!("option '{name}' requires a value"),
                ));
            }
            if *integral {
                whole(0, u32::MAX)?;
            } else {
                number("a finite number")?;
            }
            self.ignore(setting, IGNORED_BY_C_REASON);
            return Ok(());
        }
        match name {
            "post" | "ingold" => {
                self.ignore(setting, UNREAD_REASON);
                return Ok(());
            }
            "indverbosity" => {
                if setting.value.is_none() {
                    return Err(SpiceError::parse(
                        location.clone(),
                        "option 'indverbosity' requires an integer value",
                    ));
                }
                whole(0, u32::MAX)?;
                self.ignore(setting, INDVERBOSITY_REASON);
                return Ok(());
            }
            "noopiter" => {
                if setting.value.is_some() {
                    return Err(SpiceError::parse(
                        location.clone(),
                        "option 'noopiter' is a flag and takes no value",
                    ));
                }
                self.dc.noopiter = true;
                self.applied.push(AppliedOption {
                    name: setting.name.clone(),
                    value: String::new(),
                    location: location.clone(),
                });
                return Ok(());
            }
            "bypass" => {
                if setting.value.is_none() {
                    return Err(SpiceError::parse(
                        location.clone(),
                        "option 'bypass' requires an integer value",
                    ));
                }
                if whole(0, u32::MAX)? != 0 {
                    return Err(SpiceError::not_yet_ported(
                        format!("{location}: option 'bypass' other than 0 (device bypass)"),
                        C_REFERENCE,
                    ));
                }
                self.ignore(setting, BYPASS_REASON);
                return Ok(());
            }
            _ => {}
        }
        let supported = matches!(
            name,
            "temp"
                | "tnom"
                | "gmin"
                | "reltol"
                | "vntol"
                | "abstol"
                | "chgtol"
                | "trtol"
                | "method"
                | "maxord"
                | "xmu"
                | "itl1"
                | "itl2"
                | "itl4"
                | "srcsteps"
                | "itl6"
                | "gminsteps"
                | "gminfactor"
        );
        if !supported {
            if KNOWN_UNIMPLEMENTED.contains(&name) {
                return Err(SpiceError::not_yet_ported(
                    format!("{location}: option '{name}'"),
                    C_REFERENCE,
                ));
            }
            if FRONTEND_UNIMPLEMENTED.contains(&name) {
                return Err(SpiceError::not_yet_ported(
                    format!("{location}: front-end option '{name}'"),
                    FRONTEND_REFERENCE,
                ));
            }
            return Err(SpiceError::parse(
                location.clone(),
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
                location.clone(),
                format!("option '{name}' requires a value"),
            ));
        };
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
                let order = number("an integer")
                    .ok()
                    .and_then(integer_value)
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
            "itl1" | "itl2" | "itl4" => {
                // niiter.c: `if (maxIter < 100) maxIter = 100;` applies to all
                // three (CKTop, dctrcurv and dctran call NIiter with them), so
                // C's effective limit is never below 100.
                let written = whole(0, crate::analysis::newton::MAX_ITERATIONS as u32)?;
                let count = written.max(NIITER_MIN_ITERATIONS);
                match name {
                    "itl1" => self.dc.itl1 = Some(count),
                    "itl2" => {
                        self.dc.itl2 = Some(count);
                        // The adaptive strategies divide the written value
                        // (`itl2 / 4`); a zero would make every stage "slow".
                        self.dc.itl2_written = Some(written.max(1));
                    }
                    _ => self.transient.itl4 = Some(count),
                }
            }
            "srcsteps" | "itl6" => {
                self.dc.srcsteps = Some(whole(0, crate::analysis::bias::MAX_SOURCE_STEPS as u32)?);
            }
            "gminsteps" => {
                self.dc.gminsteps = Some(whole(0, crate::analysis::bias::MAX_GMIN_STAGES as u32)?);
            }
            "gminfactor" => self.dc.gminfactor = Some(number("a finite number")?),
            "xmu" => {
                let xmu = number("a finite number")?;
                if !(0. ..=0.5).contains(&xmu) {
                    return Err(SpiceError::parse(
                        value.location.clone(),
                        format!("option 'xmu' must be in [0, 0.5], not {xmu}"),
                    ));
                }
                self.transient.xmu = Some(xmu);
            }
            "gmin" => {
                let gmin = number("a finite number")?;
                if gmin < 0. {
                    return Err(SpiceError::parse(
                        value.location.clone(),
                        format!("option 'gmin' must be >= 0, not {gmin}"),
                    ));
                }
                self.context.gmin = gmin;
            }
            "temp" => {
                self.context.temperature = temperature(number("a finite number")?, location, name)?;
            }
            "tnom" => {
                self.context.nominal_temperature =
                    temperature(number("a finite number")?, location, name)?;
            }
            _ => {
                let number = number("a finite number")?;
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
        self.applied.push(AppliedOption {
            name: setting.name.clone(),
            value: value.text.clone(),
            location: location.clone(),
        });
        Ok(())
    }

    /// Temperatures and junction `gmin` shared by all analyses; defaults are
    /// 27 C / 27 C / 1e-12 S.
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
    /// defaults of [`crate::analysis::bias::DcSettings`]).
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

    /// Accepted options with an effect, in deck order (duplicates included).
    #[must_use]
    pub fn applied(&self) -> &[AppliedOption] {
        &self.applied
    }

    /// Accepted documented no-op options, in deck order, with the reason each
    /// has no effect (see the module documentation).
    #[must_use]
    pub fn ignored(&self) -> &[IgnoredOption] {
        &self.ignored
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
        let literal = crate::netlist::elaborate::literalize_analysis(
            card,
            scope,
            &mut crate::netlist::eval::EvalBudget::default(),
        )?;
        self.request(AnalysisRequest::from(&literal))
    }

    /// Add deck settings to a request. Explicit request arguments win.
    ///
    /// * `.op`, `.dc`, `.ac`, `.tf`, `.sp`: `reltol`/`vntol`/`abstol` (as `rtol`/`vntol`/
    ///   `abstol`), `itl1` (as `maxiter`), `srcsteps`, `gminsteps`,
    ///   `gminfactor` and, when the deck sets `itl1` or `itl2`, the
    ///   continuation stage limit `stagemaxiter` (`itl2`, else C's effective
    ///   default 100); `.dc` also `itl2` (as `trcvmaxiter`).
    /// * `.tran`: the tolerances plus `chgtol`, `trtol`, `method`, `maxord`,
    ///   `itl4` (as `tranmaxiter`), `xmu` and the initial-bias DC options
    ///   (`maxiter`, `stagemaxiter`, `srcsteps`, `gminsteps`, `gminfactor`)
    ///   for the companion driver.
    ///
    /// # Errors
    /// [`SpiceError::Unsupported`] when the deck selected `method`/`maxord`,
    /// `chgtol`/`trtol`, `itl4`, `xmu` or a DC option and the transient request
    /// names `backend=diffsol`, which implements none of them, so the selection
    /// cannot be honoured and is not silently ignored.
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
                AnalysisKind::OperatingPoint
                    | AnalysisKind::DcSweep
                    | AnalysisKind::Ac
                    | AnalysisKind::TransferFunction
                    | AnalysisKind::SParameter
            ) {
                for (key, value) in [
                    ("rtol", self.transient.rtol),
                    ("vntol", self.transient.vntol),
                    ("abstol", self.transient.abstol),
                ] {
                    push_real(&mut request, key, value);
                }
                self.push_dc(&mut request);
                if request.kind == AnalysisKind::DcSweep {
                    push_count(
                        &mut request,
                        crate::analysis::sweep::POINT_ITERATIONS_KEY,
                        self.dc.itl2,
                    );
                }
            }
            return Ok(request);
        }
        let diffsol = request
            .named("backend")
            .is_some_and(|name| name.eq_ignore_ascii_case("diffsol"));
        if diffsol
            && let Some(option) = self.applied.iter().find(|option| {
                DC_OPTIONS.contains(&option.name.as_str())
                    || COMPANION_ONLY.contains(&option.name.as_str())
            })
        {
            return Err(SpiceError::Unsupported {
                feature: format!(
                    ".option {}: backend=diffsol implements neither the companion Newton/\
                     trapezoidal settings nor the DC initial-bias continuation options \
                     (omit backend= to run the companion driver)",
                    option.name
                ),
                location: Some(option.location.clone()),
            });
        }
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
            ("xmu", self.transient.xmu),
        ] {
            push_real(&mut request, key, value);
        }
        push_count(&mut request, "tranmaxiter", self.transient.itl4);
        self.push_dc(&mut request);
        Ok(request)
    }

    /// DC Newton/continuation settings shared by `.op`/`.dc`/`.ac` and the
    /// companion transient initial bias.
    ///
    /// C (`cktop.c`) bounds the direct solve with `itl1` and every gmin/source
    /// stepping stage with `itl2`, so once the deck sets either, the stages get
    /// `itl2` (or C's effective default 100) instead of following `itl1`.
    /// Without deck limits the port's own defaults apply to all stages.
    fn push_dc(&self, request: &mut AnalysisRequest) {
        push_count(request, "maxiter", self.dc.itl1);
        if self.dc.itl1.is_some() || self.dc.itl2.is_some() {
            push_count(
                request,
                crate::analysis::newton::STAGE_ITERATIONS_KEY,
                Some(self.dc.itl2.unwrap_or(C_DEFAULT_ITL2_EFFECTIVE)),
            );
        }
        push_count(request, "srcsteps", self.dc.srcsteps);
        push_count(request, "gminsteps", self.dc.gminsteps);
        push_real(request, "gminfactor", self.dc.gminfactor);
        // The written itl2 steers only ngspice's adaptive strategies; a
        // request that selects the port's fixed ladders has none to steer.
        let ladder = request
            .named(crate::analysis::newton::SCHEDULE_KEY)
            .is_some_and(|schedule| schedule.trim().eq_ignore_ascii_case("ladder"));
        if !ladder {
            push_count(
                request,
                crate::analysis::newton::ADAPT_ITERATIONS_KEY,
                self.dc.itl2_written,
            );
        }
        if self.dc.noopiter {
            push_count(request, crate::analysis::newton::SKIP_DIRECT_KEY, Some(1));
        }
    }
}

/// Whether `apply` can accept `name` (with an effect or as a no-op).
fn accepted(name: &str) -> bool {
    PRINT_FLAGS.contains(&name)
        || IGNORED_BY_C.iter().any(|(ignored, _)| *ignored == name)
        || matches!(
            name,
            "post"
                | "ingold"
                | "indverbosity"
                | "bypass"
                | "temp"
                | "tnom"
                | "gmin"
                | "reltol"
                | "vntol"
                | "abstol"
                | "chgtol"
                | "trtol"
                | "method"
                | "maxord"
                | "xmu"
                | "itl1"
                | "itl2"
                | "itl4"
                | "srcsteps"
                | "itl6"
                | "gminsteps"
                | "gminfactor"
        )
}

fn push_real(request: &mut AnalysisRequest, key: &str, value: Option<Real>) {
    if let (Some(value), None) = (value, request.named(key)) {
        request.arguments.push(format!("{key}={value:e}"));
    }
}

fn push_count(request: &mut AnalysisRequest, key: &str, value: Option<usize>) {
    if let (Some(value), None) = (value, request.named(key)) {
        request.arguments.push(format!("{key}={value}"));
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

fn integer_value(value: Real) -> Option<u32> {
    (value.is_finite() && value.fract() == 0. && (0. ..=f64::from(u32::MAX)).contains(&value))
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
