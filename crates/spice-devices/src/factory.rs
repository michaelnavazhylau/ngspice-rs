//! Scalar linear-device elaboration; unsupported parameters never disappear.
use crate::{Capacitor, Device, IndependentSource, Inductor, PulseSpec, Resistor, Waveform};
use spice_core::{Complex, NodeTable, SpiceError, SpiceResult, parse_spice_number};
use spice_netlist::source::{Deck, LogicalLine};
use spice_netlist::{
    Parser, RawCard,
    ast::{DeviceInstance, ParameterKind, PositionedValue, SourceWaveform},
};

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

/// Resolve and validate before any node interning. Bounded R/C/L and M4 D/Q/M
/// factories reject all unsupported physics before committing staged nodes.
pub(crate) fn instantiate_with_models(
    instance: &DeviceInstance,
    nodes: &mut NodeTable,
    models: &crate::models::ModelResolver<'_>,
    context: &crate::models::ModelContext,
) -> SpiceResult<Box<dyn Device>> {
    context.validate(&instance.location)?;
    if let Some(model) = models.resolve(instance)? {
        if matches!(
            model.family(),
            crate::models::ModelFamily::Resistor
                | crate::models::ModelFamily::Capacitor
                | crate::models::ModelFamily::Inductor
        ) {
            return crate::passive::instantiate(instance, nodes, &model, context);
        }
        if model.family() == crate::models::ModelFamily::Diode {
            return crate::nonlinear::Diode::instantiate(instance, nodes, &model, context);
        }
        match model.family() {
            crate::models::ModelFamily::Npn | crate::models::ModelFamily::Pnp => {
                return crate::transistors::Bjt::instantiate(instance, nodes, &model, context);
            }
            crate::models::ModelFamily::Nmos | crate::models::ModelFamily::Pmos => {
                return crate::transistors::Mos1::instantiate(instance, nodes, &model, context);
            }
            _ => {}
        }
        let reference = match model.family().designator() {
            'r' => "src/spicelib/devices/res/restemp.c, ressetup.c",
            'c' => "src/spicelib/devices/cap/captemp.c",
            'l' => "src/spicelib/devices/ind/indtemp.c",
            'd' => "src/spicelib/devices/dio/dioload.c",
            'q' => "src/spicelib/devices/bjt/bjtload.c",
            _ => "src/spicelib/devices/mos1/mos1load.c",
        };
        return Err(SpiceError::not_yet_ported(
            format!(
                "{}: model-backed {:?} factory for '{}'",
                instance.location,
                model.family(),
                instance.name
            ),
            reference,
        ));
    }
    instantiate(instance, nodes)
}

pub(crate) fn instantiate(
    instance: &DeviceInstance,
    nodes: &mut NodeTable,
) -> SpiceResult<Box<dyn Device>> {
    if matches!(instance.designator, 'e' | 'f' | 'g' | 'h') {
        return crate::controlled::instantiate(instance, nodes);
    }
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
    // C applies waveform setters in order, so the last one wins.
    let mut waveform = None;
    for p in &instance.parameters {
        if let (ParameterKind::Waveform(source), 'v' | 'i') = (&p.kind, instance.designator) {
            waveform = Some(source_waveform(&p.name, source)?);
            continue;
        }
        if p.kind != ParameterKind::Scalar {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "{}: {} setter '{}' runtime semantics",
                    p.location, instance.name, p.name
                ),
                match instance.designator {
                    'v' => "src/spicelib/devices/vsrc/vsrcload.c",
                    'i' => "src/spicelib/devices/isrc/isrcload.c",
                    _ => "src/spicelib/parser/inpdpar.c",
                },
            ));
        }
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
    // An explicit DC value is kept apart from time forcing. Without one, DC
    // analyses see the waveform's time-zero level (vsrcload.c evaluates the
    // transient function at time 0 when DC is not given).
    let value = match (value, &waveform) {
        (Some(v), _) => v,
        (None, Some(w)) => initial_level(w)?,
        (None, None) => 0.,
    };
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
            waveform.unwrap_or(Waveform::Constant(value)),
        )?),
    };
    *nodes = new_nodes;
    Ok(device)
}

/// Numeric text of a waveform field. `what` names the field in diagnostics.
fn field(value: &PositionedValue, what: &str) -> SpiceResult<f64> {
    parse_spice_number(&value.text)
        .filter(|v| v.is_finite())
        .ok_or_else(|| SpiceError::Unsupported {
            feature: format!("non-finite or nonliteral {what}={}", value.text),
            location: Some(value.location.clone()),
        })
}

/// Converts a parsed PULSE/PWL setter into a device waveform. Analysis-dependent
/// PULSE defaults stay pending (`vsrcload.c`, `case PULSE`).
fn source_waveform(name: &str, source: &SourceWaveform) -> SpiceResult<Waveform> {
    match (name, source) {
        ("pulse", SourceWaveform::Pulse(p)) => {
            let optional = |v: &Option<PositionedValue>, what| v.as_ref().map(|v| field(v, what));
            let spec = PulseSpec {
                initial: field(&p.initial, "pulse v1")?,
                pulsed: field(&p.pulsed, "pulse v2")?,
                delay: optional(&p.delay, "pulse td").transpose()?,
                rise: optional(&p.rise, "pulse tr").transpose()?,
                fall: optional(&p.fall, "pulse tf").transpose()?,
                width: optional(&p.width, "pulse pw").transpose()?,
                period: optional(&p.period, "pulse per").transpose()?,
            };
            let waveform = Waveform::PulseDefaults(spec);
            waveform
                .validate()
                .map_err(|e| waveform_error(&p.initial, &e))?;
            Ok(waveform)
        }
        ("pwl", SourceWaveform::Pwl(points)) => {
            let knots = points
                .iter()
                .map(|p| Ok((field(&p.time, "pwl time")?, field(&p.value, "pwl value")?)))
                .collect::<SpiceResult<Vec<_>>>()?;
            let waveform = Waveform::Pwl(knots);
            if let Some(first) = points.first() {
                waveform.validate().map_err(|_| SpiceError::Unsupported {
                    feature: "PWL times must be nonnegative and strictly increasing".to_string(),
                    location: Some(first.time.location.clone()),
                })?;
            }
            Ok(waveform)
        }
        _ => Err(SpiceError::not_yet_ported(
            format!("source waveform '{name}'"),
            "src/spicelib/devices/vsrc/vsrcload.c",
        )),
    }
}

fn waveform_error(at: &PositionedValue, error: &SpiceError) -> SpiceError {
    SpiceError::Unsupported {
        feature: error.to_string(),
        location: Some(at.location.clone()),
    }
}

/// DC level used when no explicit DC value accompanies a waveform: the time-zero
/// value (V1 for PULSE; the first level for PWL).
fn initial_level(waveform: &Waveform) -> SpiceResult<f64> {
    match waveform {
        Waveform::PulseDefaults(spec) => Ok(spec.initial),
        other => other.value_at(0., crate::Limit::Right),
    }
}
