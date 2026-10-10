//! MOS model binning: choosing one `.model <name>.<n>` card by instance L/W.
//!
//! C: `spicelib/parser/inpgmod.c::INPgetModBin` (selection, `in_range`,
//! `parse_line`), `misc/string.c::model_name_match` (the `.<digits>` suffix),
//! `spicelib/parser/inp2m.c` (exact lookup first, binning only as a fallback)
//! and `frontend/subckt.c` (scoped renaming with the same name match).
//!
//! The rules below were read from those files and confirmed against the C
//! binary (see `docs/port/MODEL_SCHEMAS.md`, "Model binning"):
//!
//! - Only an `M` instance is binned, and only when no model has the exact
//!   referenced name. A candidate is a model named `<name>.<digits>` (at least
//!   one digit, nothing else; `nch.01` matches `nch`, `nch.a` and `nch.` do not).
//! - Only BSIM3 (`level` 8/49), BSIM4 (14/54), HiSIM2 (68) and HiSIM-HV (73)
//!   `nmos`/`pmos`/`nsoi`/`psoi` cards are binnable; other candidates are
//!   skipped, so level-1 `.N` cards never bin and C reports
//!   "could not find a valid modelname".
//! - A candidate must set all four of `lmin lmax wmin wmax` (last setter wins,
//!   as `parse_line` overwrites); otherwise it is skipped.
//! - The instance must write both `l` and `w`; model or option defaults
//!   (`defl`/`defw`) do not take part. `L = l*scale`, `W = w/nf*scale`, where the
//!   `nf` divisor applies only when the instance sets `nf` and either the
//!   instance's `wnflag` is nonzero or, without an instance `wnflag`, the option
//!   `wnflag` is set (default 0 outside HSPICE/Spectre compatibility). The
//!   multiplier `m` never takes part.
//! - A bound matches when `min < v < max` or `|v - min| < 1e-9` or
//!   `|v - max| < 1e-9` (absolute, in metres): both edges are inclusive, so
//!   adjacent bins overlap at their shared edge.
//! - Candidates are tried in C's model-table order, which is *reverse*
//!   declaration order (`INPmakeMod` prepends; a duplicate name keeps its first
//!   declaration), so the last declared matching bin wins. Overlap is not an
//!   error in C and is not one here.
//! - No match, missing L/W or no binnable candidate is a C error.
//!
//! This module only selects a card; it never picks a backend. Whether the
//! selected card can be simulated is decided by the ordinary level selector in
//! [`crate::devices::models`], which keeps unported families `NotYetPorted`.

use crate::netlist::ast::{DeviceInstance, ModelCard, ParameterAssignment};
use crate::primitives::{Real, SpiceError, SpiceResult};

use crate::devices::schema::finite_literal;

/// Absolute bound tolerance of `inpgmod.c::is_equal` (metres).
pub const BIN_TOLERANCE: Real = 1e-9;

/// MOS `level` selectors whose C model type `INPgetModBin` accepts:
/// BSIM3 (8, 49), BSIM4 (14, 54), HiSIM2 (68) and HiSIM-HV (73).
pub const BINNABLE_MOS_LEVELS: [u8; 6] = [8, 49, 14, 54, 68, 73];

/// How a model name relates to a reference (`misc/string.c::model_name_match`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameMatch {
    /// The names are equal.
    Exact,
    /// The model is `<reference>.<digits>`, a binning candidate.
    Bin,
}

/// Case-insensitive `model_name_match`: `None` when `model_name` is neither the
/// reference nor `<reference>.<one or more ASCII digits>`.
#[must_use]
pub fn model_name_match(reference: &str, model_name: &str) -> Option<NameMatch> {
    let (reference, model_name) = (reference.as_bytes(), model_name.as_bytes());
    if model_name.len() < reference.len()
        || !model_name[..reference.len()].eq_ignore_ascii_case(reference)
    {
        return None;
    }
    match &model_name[reference.len()..] {
        [] => Some(NameMatch::Exact),
        [b'.', digits @ ..] if !digits.is_empty() && digits.iter().all(u8::is_ascii_digit) => {
            Some(NameMatch::Bin)
        }
        _ => None,
    }
}

/// Whether C would consider this card for binning: an `nmos`/`pmos`/`nsoi`/
/// `psoi` card whose first `level` rounds (as `INPfindLev`) into
/// [`BINNABLE_MOS_LEVELS`]. A missing level is 1. Version strings are not
/// inspected: every BSIM3/BSIM4/HiSIM-HV version C knows is binnable.
#[must_use]
pub fn is_binnable(card: &ModelCard) -> bool {
    let mos = ["nmos", "pmos", "nsoi", "psoi"]
        .iter()
        .any(|base| card.base.eq_ignore_ascii_case(base));
    let level = card.level.unwrap_or(1.);
    let rounded = (level + 0.5).floor();
    mos && level.is_finite()
        && (0.0..=99.0).contains(&rounded)
        && BINNABLE_MOS_LEVELS.contains(&(rounded as u8))
}

/// Front-end settings that `INPgetModBin` reads from the option table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BinOptions {
    /// `.options scale` applied to the instance `l`/`w` (default 1).
    pub scale: Real,
    /// `.options wnflag`: divide `w` by the instance `nf` when the instance has
    /// no `wnflag` of its own (default off; on only in HSPICE/Spectre
    /// compatibility, which this port does not model).
    pub wnflag: bool,
}

impl Default for BinOptions {
    fn default() -> Self {
        Self {
            scale: 1.,
            wnflag: false,
        }
    }
}

/// Effective instance geometry compared against the bin bounds (metres).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BinGeometry {
    /// `l * scale`.
    pub length: Real,
    /// `w / divisor * scale`, with the `nf` divisor rule of the module docs.
    pub width: Real,
}

impl BinGeometry {
    /// The geometry `INPgetModBin` derives from an instance's written `l`, `w`,
    /// `nf` and `wnflag` setters (the last of each wins). `Ok(None)` when `l`
    /// or `w` is not written on the instance, which C treats as "no bin".
    ///
    /// # Errors
    /// Non-literal or nonfinite values, a nonpositive `nf` divisor, a
    /// nonfinite/nonpositive scale, or a nonfinite derived geometry.
    pub fn from_instance(
        instance: &DeviceInstance,
        options: &BinOptions,
    ) -> SpiceResult<Option<Self>> {
        if !(options.scale.is_finite() && options.scale > 0.) {
            return Err(SpiceError::parse(
                instance.location.clone(),
                format!(
                    "binning scale must be finite and positive, got {}",
                    options.scale
                ),
            ));
        }
        let (Some(l), Some(w)) = (
            last(&instance.parameters, "l"),
            last(&instance.parameters, "w"),
        ) else {
            return Ok(None);
        };
        let (l, w) = (finite_literal(l)?, finite_literal(w)?);
        let divisor = match last(&instance.parameters, "nf") {
            None => 1.,
            Some(nf) => {
                let divide = match last(&instance.parameters, "wnflag") {
                    Some(flag) => finite_literal(flag)? != 0.,
                    None => options.wnflag,
                };
                let nf_value = finite_literal(nf)?;
                if !divide {
                    1.
                } else if nf_value > 0. {
                    nf_value
                } else {
                    return Err(SpiceError::parse(
                        nf.location.clone(),
                        format!("binning width divisor nf must be positive, got {nf_value}"),
                    ));
                }
            }
        };
        let geometry = Self {
            length: l * options.scale,
            width: w / divisor * options.scale,
        };
        if !(geometry.length.is_finite() && geometry.width.is_finite()) {
            return Err(SpiceError::parse(
                instance.location.clone(),
                "binning geometry overflowed",
            ));
        }
        Ok(Some(geometry))
    }
}

/// One candidate's `LMIN LMAX WMIN WMAX` (metres, unscaled as in C).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BinBounds {
    /// `lmin`.
    pub lmin: Real,
    /// `lmax`.
    pub lmax: Real,
    /// `wmin`.
    pub wmin: Real,
    /// `wmax`.
    pub wmax: Real,
}

impl BinBounds {
    /// The four bounds from a card, last setter of each winning. `Ok(None)`
    /// when any of them is absent: C skips such a candidate.
    ///
    /// # Errors
    /// A present bound that is not a finite scalar literal.
    pub fn from_card(card: &ModelCard) -> SpiceResult<Option<Self>> {
        let mut values = [0.; 4];
        for (slot, name) in values.iter_mut().zip(["lmin", "lmax", "wmin", "wmax"]) {
            let Some(parameter) = last(&card.parameters, name) else {
                return Ok(None);
            };
            *slot = finite_literal(parameter)?;
        }
        let [lmin, lmax, wmin, wmax] = values;
        Ok(Some(Self {
            lmin,
            lmax,
            wmin,
            wmax,
        }))
    }

    /// `inpgmod.c::in_range` on both dimensions.
    #[must_use]
    pub fn contains(&self, geometry: BinGeometry) -> bool {
        in_range(geometry.length, self.lmin, self.lmax)
            && in_range(geometry.width, self.wmin, self.wmax)
    }
}

/// `inpgmod.c::in_range`: `min <= value <= max` with [`BIN_TOLERANCE`] at
/// both (inclusive) edges.
#[must_use]
pub fn in_range(value: Real, min: Real, max: Real) -> bool {
    (value - min).abs() < BIN_TOLERANCE
        || (value - max).abs() < BIN_TOLERANCE
        || (min < value && value < max)
}

/// Result of [`select_bin`]; the non-`Selected` outcomes are C errors that
/// the caller reports with its own location context.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOutcome<'a> {
    /// The card C would bind.
    Selected(&'a ModelCard),
    /// No candidate is a binnable family/level.
    NoBinnableCandidate,
    /// The instance does not write both `l` and `w`.
    MissingGeometry,
    /// No binnable candidate with all four bounds contains the geometry.
    NoMatch(BinGeometry),
}

/// Choose among `candidates` (first declarations, in declaration order) as
/// `INPgetModBin` does: reverse declaration order, first binnable candidate
/// with four bounds containing the instance geometry.
///
/// # Errors
/// Invalid instance geometry or bound values (see [`BinGeometry`],
/// [`BinBounds`]).
pub fn select_bin<'a>(
    candidates: &[&'a ModelCard],
    instance: &DeviceInstance,
    options: &BinOptions,
) -> SpiceResult<BinOutcome<'a>> {
    let binnable: Vec<&'a ModelCard> = candidates
        .iter()
        .copied()
        .filter(|card| is_binnable(card))
        .collect();
    if binnable.is_empty() {
        return Ok(BinOutcome::NoBinnableCandidate);
    }
    let Some(geometry) = BinGeometry::from_instance(instance, options)? else {
        return Ok(BinOutcome::MissingGeometry);
    };
    for card in binnable.into_iter().rev() {
        if BinBounds::from_card(card)?.is_some_and(|bounds| bounds.contains(geometry)) {
            return Ok(BinOutcome::Selected(card));
        }
    }
    Ok(BinOutcome::NoMatch(geometry))
}

fn last<'p>(parameters: &'p [ParameterAssignment], name: &str) -> Option<&'p ParameterAssignment> {
    parameters
        .iter()
        .rev()
        .find(|p| p.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netlist::ast::ParameterKind;
    use crate::primitives::SourceLoc;
    use std::path::PathBuf;

    fn loc(line: u32) -> SourceLoc {
        SourceLoc::new(PathBuf::from("bins.cir"), line, 1)
    }

    fn scalar(name: &str, value: &str) -> ParameterAssignment {
        ParameterAssignment {
            name: name.into(),
            value: value.into(),
            kind: ParameterKind::Scalar,
            location: loc(1),
        }
    }

    fn card(name: &str, base: &str, level: Real, bounds: &[(&str, &str)]) -> ModelCard {
        let mut parameters = vec![scalar("level", &level.to_string())];
        parameters.extend(bounds.iter().map(|(n, v)| scalar(n, v)));
        ModelCard {
            name: name.into(),
            base: base.into(),
            level: Some(level),
            parameters,
            location: loc(2),
        }
    }

    fn bsim3(name: &str, l: (&str, &str), w: (&str, &str)) -> ModelCard {
        card(
            name,
            "nmos",
            8.,
            &[("lmin", l.0), ("lmax", l.1), ("wmin", w.0), ("wmax", w.1)],
        )
    }

    fn mos(parameters: &[(&str, &str)]) -> DeviceInstance {
        DeviceInstance {
            name: "m1".into(),
            designator: 'm',
            nodes: Vec::new(),
            model: Some("nch".into()),
            parameters: parameters.iter().map(|(n, v)| scalar(n, v)).collect(),
            location: loc(3),
        }
    }

    fn selected<'a>(outcome: BinOutcome<'a>) -> &'a str {
        match outcome {
            BinOutcome::Selected(card) => &card.name,
            other => panic!("expected a bin, got {other:?}"),
        }
    }

    #[test]
    fn name_match_requires_a_dot_and_only_digits() {
        assert_eq!(model_name_match("nch", "nch"), Some(NameMatch::Exact));
        assert_eq!(model_name_match("NCH", "nch.1"), Some(NameMatch::Bin));
        assert_eq!(model_name_match("nch", "nch.01"), Some(NameMatch::Bin));
        assert_eq!(
            model_name_match("x1.nch", "x1.nch.12"),
            Some(NameMatch::Bin)
        );
        for other in [
            "nch.", "nch.a", "nch.1a", "nch1", "nchx.1", "nc", "x1.nch.1",
        ] {
            assert_eq!(model_name_match("nch", other), None, "{other}");
        }
    }

    #[test]
    fn only_bsim_and_hisim_levels_are_binnable() {
        for level in [8., 49., 14., 54., 68., 73., 7.6, 48.5] {
            assert!(is_binnable(&card("n.1", "nmos", level, &[])), "{level}");
        }
        assert!(is_binnable(&card("n.1", "PMOS", 54., &[])));
        assert!(is_binnable(&card("n.1", "nsoi", 8., &[])));
        for level in [1., 2., 3., 6., 9., 10., 55., 0., f64::NAN, 1e9] {
            assert!(!is_binnable(&card("n.1", "nmos", level, &[])), "{level}");
        }
        assert!(!is_binnable(&card("n.1", "npn", 8., &[])));
        let mut missing = card("n.1", "nmos", 8., &[]);
        missing.level = None;
        assert!(!is_binnable(&missing));
    }

    #[test]
    fn edges_are_inclusive_with_an_absolute_nanometre_tolerance() {
        assert!(in_range(1e-6, 1e-6, 2e-6));
        assert!(in_range(2e-6, 1e-6, 2e-6));
        assert!(in_range(2e-6 + 0.9e-9, 1e-6, 2e-6));
        assert!(!in_range(2e-6 + 1.1e-9, 1e-6, 2e-6));
        assert!(!in_range(1e-6 - 1.1e-9, 1e-6, 2e-6));
        assert!(in_range(1.5e-6, 1e-6, 2e-6));
    }

    #[test]
    fn last_declared_matching_bin_wins_like_the_c_model_table() {
        let first = bsim3("nch.1", ("0.5u", "1u"), ("0.5u", "5u"));
        let second = bsim3("nch.2", ("1u", "5u"), ("0.5u", "5u"));
        let edge = mos(&[("w", "1u"), ("l", "1u")]);
        let options = BinOptions::default();
        let forward = select_bin(&[&first, &second], &edge, &options).unwrap();
        assert_eq!(selected(forward), "nch.2");
        let reverse = select_bin(&[&second, &first], &edge, &options).unwrap();
        assert_eq!(selected(reverse), "nch.1");
        let inside = mos(&[("w", "1u"), ("l", "0.7u")]);
        let only = select_bin(&[&first, &second], &inside, &options).unwrap();
        assert_eq!(selected(only), "nch.1");
    }

    #[test]
    fn missing_geometry_bounds_and_families_follow_c() {
        let options = BinOptions::default();
        let good = bsim3("nch.1", ("0.5u", "2u"), ("0.5u", "5u"));
        let no_l = mos(&[("w", "1u")]);
        assert_eq!(
            select_bin(&[&good], &no_l, &options).unwrap(),
            BinOutcome::MissingGeometry
        );
        let instance = mos(&[("w", "1u"), ("l", "1u"), ("m", "4")]);
        let partial = card(
            "nch.2",
            "nmos",
            8.,
            &[("lmin", "0.5u"), ("lmax", "2u"), ("wmin", "0.5u")],
        );
        // A candidate lacking a bound is skipped, even though it is declared last.
        assert_eq!(
            selected(select_bin(&[&good, &partial], &instance, &options).unwrap()),
            "nch.1"
        );
        assert!(matches!(
            select_bin(&[&partial], &instance, &options).unwrap(),
            BinOutcome::NoMatch(_)
        ));
        let level1 = card(
            "nch.3",
            "nmos",
            1.,
            &[
                ("lmin", "0.5u"),
                ("lmax", "2u"),
                ("wmin", "0.5u"),
                ("wmax", "5u"),
            ],
        );
        assert_eq!(
            select_bin(&[&level1], &instance, &options).unwrap(),
            BinOutcome::NoBinnableCandidate
        );
        // A level-1 card in range never shadows a binnable one.
        assert_eq!(
            selected(select_bin(&[&good, &level1], &instance, &options).unwrap()),
            "nch.1"
        );
        let wide = mos(&[("w", "10u"), ("l", "1u")]);
        let BinOutcome::NoMatch(geometry) = select_bin(&[&good], &wide, &options).unwrap() else {
            panic!("expected no match");
        };
        assert_eq!(geometry.length, 1e-6);
        assert!((geometry.width - 1e-5).abs() < 1e-18);
    }

    #[test]
    fn last_setters_win_for_bounds_and_geometry() {
        let card = card(
            "nch.1",
            "nmos",
            8.,
            &[
                ("lmin", "5u"),
                ("lmin", "0.5u"),
                ("lmax", "2u"),
                ("wmin", "0.5u"),
                ("wmax", "5u"),
            ],
        );
        let instance = mos(&[("l", "9u"), ("w", "1u"), ("l", "1u")]);
        let outcome = select_bin(&[&card], &instance, &BinOptions::default()).unwrap();
        assert_eq!(selected(outcome), "nch.1");
    }

    #[test]
    fn nf_divides_width_only_under_wnflag_and_scale_multiplies_both() {
        let options = BinOptions::default();
        let geometry = |parameters: &[(&str, &str)], options: &BinOptions| {
            BinGeometry::from_instance(&mos(parameters), options)
                .unwrap()
                .unwrap()
        };
        let base = [("w", "4u"), ("l", "1u"), ("nf", "4")];
        assert_eq!(geometry(&base, &options).width, 4e-6);
        let on = BinOptions {
            wnflag: true,
            ..options
        };
        assert!((geometry(&base, &on).width - 1e-6).abs() < 1e-18);
        let instance_on = [("w", "4u"), ("l", "1u"), ("nf", "4"), ("wnflag", "1")];
        assert!((geometry(&instance_on, &options).width - 1e-6).abs() < 1e-18);
        let instance_off = [("w", "4u"), ("l", "1u"), ("nf", "4"), ("wnflag", "0")];
        assert_eq!(geometry(&instance_off, &on).width, 4e-6);
        // wnflag without nf changes nothing.
        let flag_only = [("w", "4u"), ("l", "1u"), ("wnflag", "1")];
        assert_eq!(geometry(&flag_only, &on).width, 4e-6);
        let scaled = BinOptions {
            scale: 1e-6,
            ..options
        };
        let g = geometry(&[("w", "4"), ("l", "1")], &scaled);
        assert_eq!((g.length, g.width), (1e-6, 4e-6));
        let zero_nf = mos(&[("w", "4u"), ("l", "1u"), ("nf", "0")]);
        assert!(BinGeometry::from_instance(&zero_nf, &on).is_err());
        assert!(BinGeometry::from_instance(&zero_nf, &options).is_ok());
        let bad_scale = BinOptions {
            scale: 0.,
            ..options
        };
        assert!(BinGeometry::from_instance(&mos(&base), &bad_scale).is_err());
    }

    #[test]
    fn non_literal_values_are_explicit_errors() {
        let mut instance = mos(&[("w", "1u"), ("l", "1u")]);
        instance.parameters[0].kind = ParameterKind::Flag;
        assert!(BinGeometry::from_instance(&instance, &BinOptions::default()).is_err());
        let bad = bsim3("nch.1", ("0.5u", "x"), ("0.5u", "5u"));
        assert!(BinBounds::from_card(&bad).is_err());
    }
}
