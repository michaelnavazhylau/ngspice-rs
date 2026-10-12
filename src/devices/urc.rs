//! Uniform distributed RC line (`U` instances, `urc` models).
//!
//! C has no URC equations: `urc/urcsetup.c::URCsetup` replaces every instance,
//! at setup time, with a ladder of ordinary resistors and capacitors (or
//! diodes when the model gives `ISPERL`), and the URC device itself has no
//! `DEVload`, `DEVacLoad` or `DEVpzLoad`. The port does the same, as a
//! *factory expansion*: [`expand`] turns one `U` instance into existing
//! [`Resistor`], [`Capacitor`] and [`crate::devices::nonlinear::Diode`]
//! devices, which [`crate::devices::Circuit::add_instances`] commits together
//! with a load-free [`UrcLine`] standing for the instance itself (C keeps the
//! URC instance next to its generated elements). OP, DC, AC, transient and
//! noise therefore see exactly the elements C simulates, through devices that
//! are already verified, and nothing URC-specific needs a stamping or state
//! hook. `.pz` is refused (C aborts it, see [`UrcLine`]) and `.sens` is
//! `NotYetPorted` (C lists zero sensitivities to the URC's own parameters).
//!
//! # Names
//!
//! `URCsetup` names everything with `IFnewUid`/`CKTmkVolt`, which join the
//! instance name and a suffix with `#`. For instance `u1` the port creates, in
//! C's order, section by section (`i = 1..=lumps`):
//!
//! - internal node `u1#hi<i>`, then `u1#lo<i>` (no `lo` in the last section,
//!   whose `lo` side is its `hi` node);
//! - resistors `u1#rlo<i>` (`lo` side) and `u1#rhi<i>` (`hi` side);
//! - capacitors `u1#clo<i>` and, except in the last section, `u1#chi<i>` to the
//!   reference terminal, or diodes `u1#dlo<i>`/`u1#dhi<i>` (anode on the line,
//!   cathode on the reference) sharing the generated model `u1#diodemod`.
//!
//! The internal nodes are ordinary circuit nodes, not device-internal ones: C's
//! default save set (`outitf.c`) keeps them, so rawfiles and plots carry
//! `v(u1#hi1)`, `v(u1#lo1)`, … exactly as ngspice writes them, and the
//! generated elements are observable as `@u1#rlo1[...]` like any resistor.
//! The instance `u1` itself answers `urcask.c`'s `@u1[l]` and `@u1[n]` (the
//! computed section count); its node-number asks are explicit gaps.
//!
//! # Section values (`urcsetup.c`)
//!
//! With `p = K`, `r0 = L*RPERL`, `c0 = L*CPERL`, `i0 = L*ISPERL`:
//!
//! - `lumps = n` when given, else `max(3, trunc(ln(wnorm*((p-1)/p)^2)/ln p))`
//!   with `wnorm = FMAX*r0*c0*2*pi`, and 3 whenever `wnorm < 35`;
//! - `r1 = r0*(p-1)/(2*p^lumps - 2)`,
//!   `c1 = c0*(p-1)/(p^(lumps-1)*(p+1) - 2)`, `i1` likewise from `i0`, and
//!   `rd = L*lumps*RSPERL` (the diode model's `RS`);
//! - section `i` uses `r = prop*r1`, `c = prop*c1` and diode `area = prop`,
//!   with `prop = p^(i-1)` accumulated by repeated multiplication as C does.
//!
//! The arithmetic follows C operation by operation (including `pow`/`log`
//! rather than integer powers), so element values match ngspice bit for bit
//! on the same platform `libm`.
//!
//! # Deliberate divergences
//!
//! C accepts degenerate inputs that it then simulates as something else, or
//! not at all. The port refuses them with explicit errors instead:
//!
//! - a missing or nonpositive `l` (C keeps length 0: zero capacitors and
//!   resistors clamped to `RESMIN`);
//! - `n < 1` (C then builds no sections, leaving the terminals unconnected);
//! - `K == 1` (C divides 0 by 0) or `K <= 0`;
//! - a nonpositive `RPERL`, a nonpositive `ISPERL` (the diode needs `IS > 0`)
//!   and, without `ISPERL`, a zero `CPERL` (zero-valued capacitors);
//! - section counts above [`MAX_LUMPS`] (a resource bound; C has none) and
//!   any section value that is not finite and positive;
//! - a `level` setter or any setter outside `URCmPTable` (C warns and ignores).
//!
//! `RSPERL` without `ISPERL` is accepted and has no effect, exactly as in C
//! (it only parameterises the generated diode model).

use std::collections::BTreeSet;

use crate::devices::linear::LinearContext;
use crate::devices::models::{ModelContext, ModelFamily, ModelResolver};
use crate::devices::rlc::{Capacitor, Resistor};
use crate::devices::schema::{ScalarDomain, ScalarParameter, ScalarSchema, ScalarUnit};
use crate::devices::traits::{Device, StampContext};
use crate::maths::Vector;
use crate::netlist::ast::{DeviceInstance, ModelCard, ParameterAssignment, ParameterKind};
use crate::primitives::{NodeId, NodeTable, Real, SourceLoc, SpiceError, SpiceResult};

/// The most sections one instance may expand into. C has no limit; the bound
/// keeps a mistyped `n=` or an extreme `FMAX` from building an unbounded
/// ladder.
pub const MAX_LUMPS: usize = 10_000;

/// `urc.c` `URCmPTable` with `urcsetup.c`'s defaults. `ISPERL` has no default:
/// whether it is given selects the diode ladder.
const MODEL: ScalarSchema<'static> = ScalarSchema {
    parameters: &[
        scalar(
            "k",
            ScalarUnit::Dimensionless,
            ScalarDomain::Positive,
            Some(1.5),
        ),
        scalar("fmax", ScalarUnit::Hertz, ScalarDomain::Finite, Some(1e9)),
        scalar(
            "rperl",
            ScalarUnit::OhmPerMetre,
            ScalarDomain::Positive,
            Some(1000.),
        ),
        scalar(
            "cperl",
            ScalarUnit::FaradPerMetre,
            ScalarDomain::NonNegative,
            Some(1e-12),
        ),
        scalar(
            "isperl",
            ScalarUnit::AmperePerMetre,
            ScalarDomain::Positive,
            None,
        ),
        scalar(
            "rsperl",
            ScalarUnit::OhmPerMetre,
            ScalarDomain::NonNegative,
            Some(0.),
        ),
    ],
};

/// `urc.c` `URCpTable` setters: `l` (`IF_REAL`) and `n` (`IF_INTEGER`, range
/// checked after rounding).
const INSTANCE: ScalarSchema<'static> = ScalarSchema {
    parameters: &[
        scalar("l", ScalarUnit::Metre, ScalarDomain::Positive, None),
        scalar("n", ScalarUnit::Dimensionless, ScalarDomain::Finite, None),
    ],
};

const fn scalar(
    name: &'static str,
    unit: ScalarUnit,
    domain: ScalarDomain,
    default: Option<Real>,
) -> ScalarParameter {
    ScalarParameter {
        name,
        unit,
        domain,
        default,
    }
}

/// The validated, expanded geometry of one URC instance (`urcsetup.c`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UrcSections {
    /// Line length `l` in metres.
    pub length: Real,
    /// Number of lumped sections.
    pub lumps: usize,
    /// Geometric ratio `K` between neighbouring sections.
    pub ratio: Real,
    /// First-section resistance `r1` in ohms.
    pub resistance: Real,
    /// First-section capacitance `c1` in farads (the diode `CJO` on the
    /// diode ladder).
    pub capacitance: Real,
    /// Diode ladder: first-section saturation current `i1` in amperes and
    /// the series resistance `rd` in ohms. `None` for the capacitor ladder.
    pub diode: Option<(Real, Real)>,
}

impl UrcSections {
    /// Applies `urcsetup.c` to validated model and instance values.
    ///
    /// `lumps` is the rounded `n=` setter, if any. `isperl` selects the diode
    /// ladder.
    ///
    /// # Errors
    /// The degenerate inputs listed in the module documentation.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        length: Real,
        lumps: Option<i64>,
        k: Real,
        fmax: Real,
        rperl: Real,
        cperl: Real,
        isperl: Option<Real>,
        rsperl: Real,
    ) -> SpiceResult<Self> {
        let invalid = |message: String| SpiceError::circuit(format!("URC: {message}"));
        if !(k.is_finite() && k > 0.) || k == 1. {
            return Err(invalid(format!(
                "K must be positive and different from 1, got {k} (urcsetup.c divides by K-1)"
            )));
        }
        let p = k;
        let r0 = length * rperl;
        let c0 = length * cperl;
        let i0 = isperl.map(|isperl| length * isperl);
        let lumps = match lumps {
            Some(given) => {
                if given < 1 {
                    return Err(invalid(format!(
                        "n={given} builds no sections (C leaves the terminals unconnected)"
                    )));
                }
                usize::try_from(given).unwrap_or(usize::MAX)
            }
            None => {
                let wnorm = fmax * r0 * c0 * 2.0 * std::f64::consts::PI;
                if wnorm < 35. {
                    3
                } else {
                    let ratio = (p - 1.) / p;
                    let count = 3.0_f64.max((wnorm * (ratio * ratio)).ln() / p.ln());
                    if !count.is_finite() || count > MAX_LUMPS as Real {
                        return Err(invalid(format!(
                            "FMAX={fmax} needs {count} sections, more than {MAX_LUMPS}"
                        )));
                    }
                    // C `(int)` truncation of a value of at least 3.
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let count = count as usize;
                    count
                }
            }
        };
        if lumps > MAX_LUMPS {
            return Err(invalid(format!(
                "{lumps} sections exceed the port's bound of {MAX_LUMPS}"
            )));
        }
        #[allow(clippy::cast_precision_loss)]
        let n = lumps as Real;
        let r1 = (r0 * (p - 1.)) / ((2. * p.powf(n)) - 2.);
        let denominator = (p.powf(n - 1.)) * (p + 1.) - 2.;
        let c1 = (c0 * (p - 1.)) / denominator;
        let diode = i0.map(|i0| ((i0 * (p - 1.)) / denominator, length * n * rsperl));
        let sections = Self {
            length,
            lumps,
            ratio: p,
            resistance: r1,
            capacitance: c1,
            diode,
        };
        // Every section value must be usable, not only the first.
        let mut prop: Real = 1.;
        for section in 1..=lumps {
            let r = prop * r1;
            let c = prop * c1;
            if !(r.is_finite() && r > 0.) {
                return Err(invalid(format!(
                    "section {section} resistance {r} is not finite and positive"
                )));
            }
            match diode {
                None if !(c.is_finite() && c > 0.) => {
                    return Err(invalid(format!(
                        "section {section} capacitance {c} is not finite and positive \
                         (CPERL must be positive without ISPERL)"
                    )));
                }
                Some((i1, rd)) => {
                    let is = prop * i1;
                    let nonnegative = |x: Real| x.is_finite() && x >= 0.;
                    let positive = |x: Real| x.is_finite() && x > 0.;
                    if !(nonnegative(c) && positive(is) && positive(prop) && nonnegative(rd)) {
                        return Err(invalid(format!(
                            "section {section} diode values (cjo={c}, is={is}, area={prop}, \
                             rs={rd}) are not usable"
                        )));
                    }
                }
                None => {}
            }
            prop *= p;
        }
        Ok(sections)
    }
}

/// Expands one `U` instance into its R/C (or R/D) ladder, in C's creation
/// order. Internal nodes are interned in `nodes` only on success.
/// `deck_nodes` are the node names the deck's own cards use: a generated
/// name among them is a collision even when its card comes later.
///
/// # Errors
/// A missing or wrong-family model, unsupported or invalid setters, the
/// degenerate inputs of [`UrcSections::new`], internal name collisions, or a
/// generated element's own validation.
pub(crate) fn expand(
    instance: &DeviceInstance,
    nodes: &mut NodeTable,
    models: &ModelResolver<'_>,
    context: &ModelContext,
    deck_nodes: &BTreeSet<&str>,
) -> SpiceResult<Vec<Box<dyn Device>>> {
    context.validate(&instance.location)?;
    if instance.nodes.len() != 3 {
        return Err(SpiceError::circuit(format!(
            "URC {} needs three terminals",
            instance.name
        )));
    }
    let Some(model) = models.resolve(instance)? else {
        return Err(SpiceError::parse(
            instance.location.clone(),
            format!("URC {} requires a model", instance.name),
        ));
    };
    if model.family() != ModelFamily::Urc {
        return Err(SpiceError::parse(
            instance.location.clone(),
            format!("URC {} needs a urc model", instance.name),
        ));
    }
    let card = model.card();
    let mut setters = Vec::with_capacity(card.parameters.len());
    for parameter in &card.parameters {
        if parameter.kind == ParameterKind::Flag && parameter.name == "urc" {
            continue; // URC_MOD_URC: a no-op type flag (urcmpar.c).
        }
        if parameter.name.eq_ignore_ascii_case("level") {
            return Err(SpiceError::Unsupported {
                feature: "level on a urc model (URCmPTable has no level; C ignores it)".into(),
                location: Some(parameter.location.clone()),
            });
        }
        setters.push(parameter);
    }
    let m = MODEL.validate(setters, &card.location)?;
    let i = INSTANCE.validate(&instance.parameters, &instance.location)?;
    let value = |name: &str| m.get(name).map(|v| v.value);
    let Some(length) = i.get("l").map(|v| v.value) else {
        return Err(SpiceError::Unsupported {
            feature: format!(
                "URC {} without l= (C uses length 0: zero-valued elements)",
                instance.name
            ),
            location: Some(instance.location.clone()),
        });
    };
    let lumps = match i.get("n") {
        None => None,
        Some(n) => Some(integer(n.value, n.location.as_ref(), &instance.location)?),
    };
    let required = |name: &str| {
        value(name).ok_or_else(|| {
            SpiceError::parse(card.location.clone(), format!("missing default for {name}"))
        })
    };
    let sections = UrcSections::new(
        length,
        lumps,
        required("k")?,
        required("fmax")?,
        required("rperl")?,
        required("cperl")?,
        value("isperl"),
        required("rsperl")?,
    )
    .map_err(|error| match error {
        SpiceError::Circuit { message } => SpiceError::parse(
            instance.location.clone(),
            format!("{}: {message}", instance.name),
        ),
        other => other,
    })?;
    build(
        instance,
        &sections,
        &card.location,
        nodes,
        context,
        deck_nodes,
    )
}

/// `INPgetValue(IF_INTEGER)`: `(int) floor(0.5 + value)`, range checked.
fn integer(value: Real, at: Option<&SourceLoc>, owner: &SourceLoc) -> SpiceResult<i64> {
    let rounded = (value + 0.5).floor();
    if !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&rounded) {
        return Err(SpiceError::Unsupported {
            feature: format!("URC n={value} is outside the C integer range"),
            location: Some(at.unwrap_or(owner).clone()),
        });
    }
    #[allow(clippy::cast_possible_truncation)]
    Ok(rounded as i64)
}

/// A scalar setter as the deck would have written it. `{:e}` is Rust's
/// shortest round-trip spelling, which `parse_spice_number` reads back
/// exactly.
fn setter(name: &str, value: Real, location: &SourceLoc) -> ParameterAssignment {
    ParameterAssignment {
        name: name.to_owned(),
        value: format!("{value:e}"),
        kind: ParameterKind::Scalar,
        location: location.clone(),
    }
}

fn build(
    instance: &DeviceInstance,
    sections: &UrcSections,
    model_location: &SourceLoc,
    nodes: &mut NodeTable,
    context: &ModelContext,
    deck_nodes: &BTreeSet<&str>,
) -> SpiceResult<Vec<Box<dyn Device>>> {
    let name = &instance.name;
    let mut staged = nodes.clone();
    // The terminals exist before setup in C (INP2U binds them at parse time).
    for terminal in &instance.nodes {
        staged.intern(terminal);
    }
    let fresh = |staged: &mut NodeTable, suffix: String| -> SpiceResult<String> {
        let node = format!("{name}#{suffix}");
        // C's CKTmkVolt fails (E_EXISTS) on a name the deck already uses,
        // whichever card comes first.
        if staged.get(&node).is_some() || deck_nodes.contains(node.as_str()) {
            return Err(SpiceError::circuit(format!(
                "URC internal-node name collision: {node}"
            )));
        }
        staged.intern(&node);
        Ok(node)
    };
    // The diode ladder's generated model (`<name>#diodemod`: cjo, rs, is set
    // in that order by URCsetup); every diode references it.
    let diode_model = sections.diode.map(|(i1, rd)| ModelCard {
        name: format!("{name}#diodemod"),
        base: "d".to_owned(),
        level: None,
        parameters: vec![
            setter("cjo", sections.capacitance, model_location),
            setter("rs", rd, model_location),
            setter("is", i1, model_location),
        ],
        location: model_location.clone(),
    });
    let diode_models = diode_model.into_iter().collect::<Vec<_>>();
    let diode_resolver = ModelResolver::new(&diode_models)?;
    let reference = instance.nodes[2].clone();
    let mut devices: Vec<Box<dyn Device>> = Vec::with_capacity(4 * sections.lumps + 1);
    // C keeps the URC instance itself (created by INP2U, before setup) next
    // to the elements URCsetup generates.
    devices.push(Box::new(UrcLine {
        name: name.clone(),
        length: sections.length,
        lumps: sections.lumps,
    }));
    let mut lowl = instance.nodes[0].clone();
    let mut hir = instance.nodes[1].clone();
    let mut prop: Real = 1.;
    let id = |staged: &mut NodeTable, node: &str| -> NodeId { staged.intern(node) };
    for section in 1..=sections.lumps {
        let hil = fresh(&mut staged, format!("hi{section}"))?;
        let lowr = if section == sections.lumps {
            hil.clone()
        } else {
            fresh(&mut staged, format!("lo{section}"))?
        };
        let r = prop * sections.resistance;
        let c = prop * sections.capacitance;
        let terminals = [id(&mut staged, &lowl), id(&mut staged, &lowr)];
        devices.push(Box::new(Resistor::new(
            format!("{name}#rlo{section}"),
            terminals,
            r,
        )?));
        let terminals = [id(&mut staged, &hil), id(&mut staged, &hir)];
        devices.push(Box::new(Resistor::new(
            format!("{name}#rhi{section}"),
            terminals,
            r,
        )?));
        let mut shunts = vec![("lo", lowr.clone())];
        if section != sections.lumps {
            shunts.push(("hi", hil.clone()));
        }
        for (side, node) in shunts {
            if sections.diode.is_some() {
                let diode = DeviceInstance {
                    name: format!("{name}#d{side}{section}"),
                    designator: 'd',
                    nodes: vec![node, reference.clone()],
                    model: Some(format!("{name}#diodemod")),
                    parameters: vec![setter("area", prop, &instance.location)],
                    location: instance.location.clone(),
                };
                let model = diode_resolver.resolve(&diode)?.ok_or_else(|| {
                    SpiceError::circuit(format!("URC {name}: generated diode model missing"))
                })?;
                devices.push(crate::devices::nonlinear::Diode::instantiate(
                    &diode,
                    &mut staged,
                    &model,
                    context,
                )?);
            } else {
                let terminals = [id(&mut staged, &node), id(&mut staged, &reference)];
                devices.push(Box::new(Capacitor::new(
                    format!("{name}#c{side}{section}"),
                    terminals,
                    c,
                    None,
                )?));
            }
        }
        prop *= sections.ratio;
        lowl = lowr;
        hir = hil;
    }
    *nodes = staged;
    Ok(devices)
}

/// The URC instance itself, as C keeps it after `URCsetup`: a device with
/// no terminals and no load (`urcinit.c`: `DEVload`, `DEVacLoad`,
/// `DEVpzLoad`, `DEVnoise` and `DEVdisto` are all `NULL`). It answers
/// `urcask.c`'s `l` and `n` asks and makes the analyses that would treat the
/// line differently from its generated elements fail explicitly.
#[derive(Debug, Clone)]
pub struct UrcLine {
    name: String,
    length: Real,
    lumps: usize,
}

impl UrcLine {
    /// The line length `l` in metres.
    #[must_use]
    pub const fn length(&self) -> Real {
        self.length
    }

    /// The number of sections the line was expanded into.
    #[must_use]
    pub const fn lumps(&self) -> usize {
        self.lumps
    }
}

impl Device for UrcLine {
    fn name(&self) -> &str {
        &self.name
    }

    fn designator(&self) -> char {
        'u'
    }

    fn terminals(&self) -> &[crate::primitives::NodeId] {
        &[]
    }

    /// No load: the generated elements carry the line.
    fn stamp(&self, _context: &mut StampContext<'_>) -> SpiceResult<()> {
        Ok(())
    }

    /// As [`Device::stamp`].
    fn assemble_linear(&self, _context: &mut LinearContext<'_>) -> SpiceResult<()> {
        Ok(())
    }

    /// C cannot run `.pz` with a URC line: `DEVpzSetup` is `URCsetup`, which
    /// tries to create the generated elements a second time and aborts the
    /// analysis ("device already exists"). The port refuses explicitly
    /// instead of producing poles C never computes.
    fn assemble_pole_zero(
        &self,
        _context: &mut LinearContext<'_>,
        _bias: &Vector,
    ) -> SpiceResult<()> {
        Err(SpiceError::Unsupported {
            feature: format!(
                "pole-zero analysis with URC line {} (C aborts: urcinit.c DEVpzSetup re-runs \
                 URCsetup, 'device already exists')",
                self.name
            ),
            location: None,
        })
    }

    /// Noiseless: `DEVnoise = NULL` (`urcinit.c`); the generated resistors
    /// and diodes carry the line's noise.
    fn noise(
        &self,
        _context: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        Ok(crate::devices::noise::DeviceNoise::Noiseless)
    }

    /// Linear in `.disto`: `DEVdisto = NULL`; the generated elements enter
    /// through their own routines.
    fn distortion(
        &self,
        _context: &crate::devices::distortion::DistortionContext<'_>,
    ) -> SpiceResult<crate::devices::distortion::DeviceDistortion> {
        Ok(crate::devices::distortion::DeviceDistortion::Linear)
    }

    /// `urcask.c`: `l` and the (possibly computed) `n`. The node-number asks
    /// (`pos_node`, `neg_node`, `gnd`) are not observable.
    fn observation_parameter(
        &self,
        keyword: &str,
        _context: &ModelContext,
    ) -> SpiceResult<Option<Real>> {
        #[allow(clippy::cast_precision_loss)]
        Ok(match keyword {
            "l" => Some(self.length),
            "n" => Some(self.lumps as Real),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sections(length: Real, lumps: Option<i64>, isperl: Option<Real>) -> UrcSections {
        UrcSections::new(length, lumps, 1.5, 1e9, 1000., 1e-12, isperl, 10.).unwrap()
    }

    #[test]
    fn short_lines_get_three_sections() {
        // wnorm = 1e9 * 50 * 5e-14 * 2 pi << 35.
        let s = sections(50e-6, None, None);
        assert_eq!(s.lumps, 3);
        // r1 = r0 (p-1) / (2 p^3 - 2), c1 = c0 (p-1) / (p^2 (p+1) - 2).
        let r0 = 50e-6 * 1000.;
        let c0 = 50e-6 * 1e-12;
        assert_eq!(s.resistance, (r0 * 0.5) / ((2. * 1.5f64.powf(3.)) - 2.));
        assert_eq!(s.capacitance, (c0 * 0.5) / (1.5f64.powf(2.) * 2.5 - 2.));
        assert!(s.diode.is_none());
    }

    #[test]
    fn total_resistance_and_capacitance_are_preserved() {
        // Both halves of the ladder sum r1 * (1 + p + ... + p^(n-1)), so the
        // series resistance is r0 exactly in real arithmetic; the shunt total
        // c1 * (2 (1 + ... + p^(n-2)) + p^(n-1)) is c0.
        for lumps in [1, 2, 3, 7] {
            let s = sections(1e-3, Some(lumps), Some(1e-15));
            let p = s.ratio;
            let mut prop = 1.;
            let (mut r, mut c) = (0., 0.);
            for section in 1..=s.lumps {
                r += 2. * prop * s.resistance;
                c += prop * s.capacitance * if section == s.lumps { 1. } else { 2. };
                prop *= p;
            }
            assert!((r - 1.).abs() < 1e-12, "{lumps}: {r}");
            assert!((c - 1e-15).abs() < 1e-27, "{lumps}: {c}");
            let (i1, rd) = s.diode.unwrap();
            assert!((i1 / s.capacitance - 1e-15 / 1e-12).abs() < 1e-12);
            assert_eq!(rd, 1e-3 * lumps as Real * 10.);
        }
    }

    #[test]
    fn long_lines_follow_the_fmax_rule() {
        // wnorm = 1e9 * 1e3 * 1e-9 * 2 pi = 6283; ln(6283 / 9) / ln 1.5 = 16.15.
        let s = UrcSections::new(1., None, 1.5, 1e9, 1e3, 1e-9, None, 0.).unwrap();
        assert_eq!(s.lumps, 16);
    }

    #[test]
    fn degenerate_inputs_are_refused() {
        let base = |k: Real, lumps: Option<i64>, cperl: Real| {
            UrcSections::new(1e-3, lumps, k, 1e9, 1e3, cperl, None, 0.)
        };
        assert!(base(1., None, 1e-12).is_err());
        assert!(base(0., None, 1e-12).is_err());
        assert!(base(1.5, Some(0), 1e-12).is_err());
        assert!(base(1.5, Some(-2), 1e-12).is_err());
        assert!(base(1.5, Some(MAX_LUMPS as i64 + 1), 1e-12).is_err());
        assert!(base(1.5, None, 0.).is_err());
        // A huge ratio overflows p^n: refused, never a zero resistor.
        assert!(base(1e300, Some(5), 1e-12).is_err());
        // K below one is legal in C: sections shrink toward the middle.
        let s = base(0.5, Some(4), 1e-12).unwrap();
        assert!(s.resistance > 0. && s.capacitance > 0.);
    }

    #[test]
    fn integer_rounding_follows_inpgval() {
        let at = SourceLoc::new(std::path::PathBuf::from("t.cir"), 1, 1);
        assert_eq!(integer(2.5, None, &at).unwrap(), 3);
        assert_eq!(integer(2.49, None, &at).unwrap(), 2);
        assert_eq!(integer(-0.5, None, &at).unwrap(), 0);
        assert!(integer(1e12, None, &at).is_err());
    }

    #[test]
    fn generated_setters_round_trip_exactly() {
        for value in [1.3793103448275862e-20, 0.1 + 0.2, 1.5f64.powf(7.), 3e-3] {
            let text = setter(
                "x",
                value,
                &SourceLoc::new(std::path::PathBuf::from("t.cir"), 1, 1),
            )
            .value;
            assert_eq!(crate::primitives::parse_spice_number(&text), Some(value));
        }
    }
}
