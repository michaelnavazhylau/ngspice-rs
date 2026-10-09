//! Instance `off` flags and initial-condition vectors of the nonlinear devices.
//!
//! C references: the instance setters of `dio/diopar.c` (`IC`, `OFF`),
//! `bjt/bjtpar.c` (`IC` vector, `ICVBE`, `ICVCE`, `OFF`) and `mos1/mos1par.c`
//! (`IC` vector, `ICVDS`, `ICVGS`, `ICVBS`, `OFF`). The setters apply in card
//! order, so a later setter of the same component wins; an `ic=` vector sets
//! only the components it lists (the parser keeps omitted ones omitted).
//!
//! How the values are used is device behaviour (`dioload.c`, `bjtload.c`,
//! `mos1load.c`, with the defaults of `diogetic.c`, `bjtgetic.c` and
//! `mos1ic.c`); see [`crate::limiting::Linearization::InitialConditions`] and
//! [`crate::limiting::Limiter::holds_off`].
use spice_core::{Real, SpiceError, SpiceResult, parse_spice_number};
use spice_netlist::ast::{ParameterAssignment, ParameterKind};

/// An instance's `off` flag and initial-condition components.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct InstanceInitial {
    /// C `*off`: the instance starts the operating point with zero junction
    /// voltages and is held there through `MODEINITFIX`.
    pub off: bool,
    /// The given components, in the order of the `components` names passed
    /// to [`split`]; `None` when not given (C `*Given` false).
    pub values: Vec<Option<Real>>,
}

fn number(text: &str, location: &spice_core::SourceLoc, name: &str) -> SpiceResult<Real> {
    parse_spice_number(text)
        .filter(|value| value.is_finite())
        .ok_or_else(|| {
            SpiceError::parse(
                location.clone(),
                format!("expected finite initial condition {name}={text}"),
            )
        })
}

/// Splits `off` and the initial-condition setters off `parameters`.
///
/// `components` are the canonical component setters (`["ic"]` for the
/// diode's scalar `ic`, `["icvbe", "icvce"]`, `["icvds", "icvgs", "icvbs"]`);
/// an `ic` vector is distributed over them by component name. Every other
/// setter is returned unchanged, in order, for the device's scalar schema.
///
/// # Errors
/// A nonfinite or nonliteral component value.
pub(crate) fn split(
    parameters: &[ParameterAssignment],
    components: &[&str],
) -> SpiceResult<(InstanceInitial, Vec<ParameterAssignment>)> {
    let mut initial = InstanceInitial {
        off: false,
        values: vec![None; components.len()],
    };
    let mut rest = Vec::new();
    for parameter in parameters {
        let name = parameter.name.to_ascii_lowercase();
        match (&parameter.kind, name.as_str()) {
            (ParameterKind::Flag, "off") => initial.off = true,
            (ParameterKind::InitialConditions(given), "ic") => {
                for component in given {
                    let index = components
                        .iter()
                        .position(|c| c.eq_ignore_ascii_case(&component.name))
                        .ok_or_else(|| SpiceError::Unsupported {
                            feature: format!("initial-condition component '{}'", component.name),
                            location: Some(component.value.location.clone()),
                        })?;
                    initial.values[index] = Some(number(
                        &component.value.text,
                        &component.value.location,
                        &component.name,
                    )?);
                }
            }
            (ParameterKind::Scalar, _) if components.contains(&name.as_str()) => {
                let index = components
                    .iter()
                    .position(|c| *c == name)
                    .unwrap_or_default();
                initial.values[index] = Some(number(
                    &parameter.value,
                    &parameter.location,
                    &parameter.name,
                )?);
            }
            _ => rest.push(parameter.clone()),
        }
    }
    Ok((initial, rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use spice_netlist::{Parser, source::parse_deck_text};
    use std::path::Path;

    fn instance(card: &str) -> Vec<ParameterAssignment> {
        Parser::new()
            .parse_deck(&parse_deck_text(
                Path::new("ic.cir"),
                &format!("t\n{card}\n.model qm npn\n.model mm nmos\n.model dm d\n.end\n"),
            ))
            .unwrap()
            .devices
            .remove(0)
            .parameters
    }

    #[test]
    fn later_setters_win_per_component_and_off_is_a_flag() {
        let (initial, rest) = split(
            &instance("q1 c b e qm off icvbe=.1 ic=.6,2 icvce=3 area=2"),
            &["icvbe", "icvce"],
        )
        .unwrap();
        assert!(initial.off);
        assert_eq!(initial.values, [Some(0.6), Some(3.)]);
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].name, "area");

        let (initial, _) = split(
            &instance("m1 d g s b mm ic=1 icvbs=-2"),
            &["icvds", "icvgs", "icvbs"],
        )
        .unwrap();
        assert!(!initial.off);
        assert_eq!(initial.values, [Some(1.), None, Some(-2.)]);

        let (initial, rest) = split(&instance("d1 a 0 dm 2 ic=.4"), &["ic"]).unwrap();
        assert_eq!(initial.values, [Some(0.4)]);
        assert_eq!(rest.len(), 1);
    }
}
