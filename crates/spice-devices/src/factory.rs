//! Scalar linear-device elaboration; unsupported parameters never disappear.
use crate::{Capacitor, Device, IndependentSource, Inductor, Resistor, Waveform};
use spice_core::{Complex, NodeTable, SpiceError, SpiceResult, parse_spice_number};
use spice_netlist::source::{Deck, LogicalLine};
use spice_netlist::{Parser, RawCard, ast::DeviceInstance};

pub(crate) fn from_card(card: &RawCard, nodes: &mut NodeTable) -> SpiceResult<Box<dyn Device>> {
    let deck = Deck {
        path: card.location.path().to_path_buf(),
        title: String::new(),
        title_location: card.location.clone(),
        lines: vec![LogicalLine {
            location: card.location.clone(),
            end_line: card.location.line,
            continuations: 0,
            text: card.raw.clone(),
        }],
    };
    let netlist = Parser::new().parse_deck(&deck)?;
    instantiate(&netlist.devices[0], nodes)
}

pub(crate) fn instantiate(
    instance: &DeviceInstance,
    nodes: &mut NodeTable,
) -> SpiceResult<Box<dyn Device>> {
    if instance.model.is_some() || instance.nodes.len() != 2 {
        return Err(SpiceError::Unsupported {
            feature: format!("model/multiport device {}", instance.name),
            location: Some(instance.location.clone()),
        });
    }
    let primary = match instance.designator {
        'r' => "resistance",
        'c' => "capacitance",
        'l' => "inductance",
        'v' | 'i' => "dc",
        _ => {
            return Err(SpiceError::not_yet_ported(
                format!("device {}", instance.name),
                "src/spicelib/devices/",
            ));
        }
    };
    let mut value = None;
    let mut ic = None;
    let mut mag = 0.;
    let mut phase: f64 = 0.;
    for p in &instance.parameters {
        let allowed = p.name == primary
            || (matches!(instance.designator, 'c' | 'l') && p.name == "ic")
            || (matches!(instance.designator, 'v' | 'i')
                && matches!(p.name.as_str(), "acmag" | "acphase"));
        if !allowed {
            return Err(SpiceError::Unsupported {
                feature: format!("{} parameter {}", instance.name, p.name),
                location: Some(p.location.clone()),
            });
        }
        let number = parse_spice_number(&p.value)
            .filter(|v| v.is_finite())
            .ok_or_else(|| SpiceError::Unsupported {
                feature: format!("non-finite or nonliteral {}={}", p.name, p.value),
                location: Some(p.location.clone()),
            })?;
        match p.name.as_str() {
            "ic" => ic = Some(number),
            "acmag" => mag = number,
            "acphase" => phase = number,
            _ => value = Some(number),
        }
    }
    let value = value.unwrap_or(0.);
    // Construct against a cloned table; failures leave the caller's node table untouched.
    let mut new_nodes = nodes.clone();
    let terminals = [
        new_nodes.intern(&instance.nodes[0]),
        new_nodes.intern(&instance.nodes[1]),
    ];
    let device: Box<dyn Device> = match instance.designator {
        'r' => Box::new(Resistor::new(&instance.name, terminals, value)?),
        'c' => Box::new(Capacitor::new(&instance.name, terminals, value, ic)?),
        'l' => Box::new(Inductor::new(&instance.name, terminals, value, ic)?),
        _ => Box::new(IndependentSource::new(
            &instance.name,
            terminals,
            instance.designator == 'v',
            value,
            Complex::new(
                mag * phase.to_radians().cos(),
                mag * phase.to_radians().sin(),
            ),
            Waveform::Constant(value),
        )?),
    };
    *nodes = new_nodes;
    Ok(device)
}
