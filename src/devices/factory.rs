//! Scalar linear-device elaboration; unsupported parameters never disappear.
use crate::devices::{
    AmSpec, Capacitor, Device, ExpSpec, FunctionSpec, IndependentSource, Inductor, PulseSpec,
    PwlSource, Resistor, SffmSpec, SineSpec, Waveform,
};
use crate::netlist::source::{Deck, LogicalLine};
use crate::netlist::{
    Parser, RawCard,
    ast::{DeviceInstance, ParameterKind, PositionedValue, SourceFunction, SourceWaveform},
};
use crate::primitives::{
    Complex, NodeTable, SourceLoc, SpiceError, SpiceResult, parse_spice_number,
};

/// Designators [`instantiate`] builds from the card alone (the registry's
/// `ported` entries). `tests/registry_support.rs` elaborates one instance of
/// every designator to keep this list and [`ELABORATED`] honest.
pub(crate) const CARD_FACTORY: &[char] = &['r', 'c', 'l', 'v', 'i', 'e', 'f', 'g', 'h', 'b', 'k'];

/// Designators built only while elaborating a deck, with the subset built:
/// D/Q/M/S/W from a resolved `.model` card ([`instantiate_with_models`]) and X
/// by subcircuit expansion (`crate::devices::subckt`). The registry's `bounded` entries.
pub(crate) const ELABORATED: &[(char, &str)] = &[
    ('d', "junction diode (dioload.c subset)"),
    ('q', "Gummel-Poon BJT level 1 (bjtload.c)"),
    ('m', "MOS1 (level 1)"),
    ('s', "voltage-controlled switch (companion .tran, no BDF)"),
    ('w', "current-controlled switch (companion .tran, no BDF)"),
    ('x', "expanded before device elaboration"),
];

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
    models: &crate::devices::models::ModelResolver<'_>,
    context: &crate::devices::models::ModelContext,
) -> SpiceResult<Box<dyn Device>> {
    context.validate(&instance.location)?;
    if let Some(model) = models.resolve(instance)? {
        if matches!(
            model.family(),
            crate::devices::models::ModelFamily::Resistor
                | crate::devices::models::ModelFamily::Capacitor
                | crate::devices::models::ModelFamily::Inductor
        ) {
            return crate::devices::passive::instantiate(instance, nodes, &model, context);
        }
        if model.family() == crate::devices::models::ModelFamily::Diode {
            return crate::devices::nonlinear::Diode::instantiate(instance, nodes, &model, context);
        }
        match model.family() {
            crate::devices::models::ModelFamily::Npn | crate::devices::models::ModelFamily::Pnp => {
                return crate::devices::bjt::Bjt::instantiate(instance, nodes, &model, context);
            }
            crate::devices::models::ModelFamily::Nmos
            | crate::devices::models::ModelFamily::Pmos => {
                return crate::devices::mos1::Mos1::instantiate(instance, nodes, &model, context);
            }
            crate::devices::models::ModelFamily::Switch
            | crate::devices::models::ModelFamily::CurrentSwitch => {
                return crate::devices::switch::Switch::instantiate(
                    instance, nodes, &model, context,
                );
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
        return crate::devices::controlled::instantiate(instance, nodes);
    }
    if instance.designator == 'k' {
        return crate::devices::mutual::instantiate(instance);
    }
    // `a` instances only come from the front end's TABLE/POLY lowering
    // (crate::netlist::behavioural); user XSPICE cards are not parsed.
    if matches!(instance.designator, 'b' | 'a') {
        return crate::devices::behavioural::instantiate(instance, nodes);
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
    let mut ac_given = false;
    // C applies waveform setters in order, so the last one wins.
    let mut waveform = None;
    // PWL `td=` (any order) and `r=` (applies to the PWL already set), as the
    // ordered VSRC_TD/VSRC_R setters of vsrcpar.c/isrcpar.c.
    let mut pwl_delay: Option<(f64, SourceLoc)> = None;
    let mut pwl_repeat: Option<(f64, SourceLoc)> = None;
    for p in &instance.parameters {
        if let (ParameterKind::Waveform(source), 'v' | 'i') = (&p.kind, instance.designator) {
            if let Some((_, at)) = &pwl_repeat {
                // C keeps r='s knot index from the earlier PWL: refuse rather
                // than apply a stale repetition point.
                return Err(SpiceError::Unsupported {
                    feature: format!(
                        "{}: waveform setter '{}' after r= (set r= after the final PWL)",
                        instance.name, p.name
                    ),
                    location: Some(at.clone()),
                });
            }
            waveform = Some(source_waveform(&p.name, source, &p.location)?);
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
                && matches!(p.name.as_str(), "acmag" | "acphase" | "r" | "td"));
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
            "acmag" => {
                mag = number;
                ac_given = true;
            }
            "acphase" => {
                phase = number;
                ac_given = true;
            }
            "td" => pwl_delay = Some((number, p.location.clone())),
            "r" => {
                // C silently ignores r= when no PWL coefficients exist yet.
                let Some(Waveform::Pwl(knots)) = &waveform else {
                    return Err(SpiceError::Unsupported {
                        feature: format!(
                            "{}: r= without a preceding PWL setter (C ignores it)",
                            instance.name
                        ),
                        location: Some(p.location.clone()),
                    });
                };
                // VSRC_R/ISRC_R validate every r= as it is set (E_PARMVAL
                // aborts the deck), so an invalid earlier r= is an error even
                // when a later one is valid.
                PwlSource::new(knots.clone(), 0., Some(number)).map_err(|error| {
                    SpiceError::Unsupported {
                        feature: format!("{}: {error}", instance.name),
                        location: Some(p.location.clone()),
                    }
                })?;
                pwl_repeat = Some((number, p.location.clone()));
            }
            _ => value = Some(number),
        }
    }
    if pwl_delay.is_some() || pwl_repeat.is_some() {
        waveform = Some(pwl_options(waveform, pwl_delay, pwl_repeat)?);
    }
    // An explicit DC value is kept apart from time forcing. Without one, DC
    // analyses see the waveform's time-zero level (vsrcload.c evaluates the
    // transient function at time 0 when DC is not given).
    let value = match (value, &waveform) {
        (Some(v), _) => v,
        (None, Some(w)) => w.time_zero()?,
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
        _ => Box::new(
            IndependentSource::new(
                &instance.name,
                terminals,
                instance.designator == 'v',
                value,
                Complex::new(
                    mag * phase.to_radians().cos(),
                    mag * phase.to_radians().sin(),
                ),
                waveform.unwrap_or(Waveform::Constant(value)),
            )?
            .with_ac_given(ac_given),
        ),
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

/// Applies PWL `td=`/`r=` to the final waveform, which must be a PWL: C ignores
/// `td=` for any other function, which the port refuses instead.
fn pwl_options(
    waveform: Option<Waveform>,
    delay: Option<(f64, SourceLoc)>,
    repeat: Option<(f64, SourceLoc)>,
) -> SpiceResult<Waveform> {
    let at = delay.as_ref().or(repeat.as_ref()).map(|(_, at)| at.clone());
    let Some(Waveform::Pwl(knots)) = waveform else {
        return Err(SpiceError::Unsupported {
            feature: "td=/r= without a PWL waveform (C ignores them)".to_owned(),
            location: at,
        });
    };
    PwlSource::new(knots, delay.map_or(0., |(d, _)| d), repeat.map(|(r, _)| r))
        .map(Waveform::PwlSource)
        .map_err(|error| SpiceError::Unsupported {
            feature: error.to_string(),
            location: at,
        })
}

/// Converts a parsed PULSE/PWL/SIN/EXP/SFFM/AM setter into a device waveform.
/// Analysis-dependent defaults stay pending (`vsrcload.c`).
/// `setter` locates the whole setter for diagnostics without a field.
fn source_waveform(
    name: &str,
    source: &SourceWaveform,
    setter: &SourceLoc,
) -> SpiceResult<Waveform> {
    match (name, source) {
        (_, SourceWaveform::Function(f)) => {
            // The parser bounds the field count; a hand-built AST may not.
            let most = f.function.fields().len();
            if !(2..=most).contains(&f.values.len()) {
                return Err(SpiceError::Unsupported {
                    feature: format!(
                        "{} with {} fields (needs 2 to {most})",
                        f.function.keyword(),
                        f.values.len()
                    ),
                    location: Some(
                        f.values
                            .get(most)
                            .map_or_else(|| setter.clone(), |extra| extra.location.clone()),
                    ),
                });
            }
            let at = &f.values[0];
            let fields = f
                .values
                .iter()
                .zip(f.function.fields())
                .map(|(value, what)| field(value, &format!("{} {what}", f.function.keyword())))
                .collect::<SpiceResult<Vec<_>>>()?;
            let get = |index: usize| fields.get(index).copied();
            let (v0, v1) = (fields[0], fields[1]);
            let spec = match f.function {
                SourceFunction::Sin => FunctionSpec::Sine(SineSpec {
                    offset: v0,
                    amplitude: v1,
                    frequency: get(2),
                    delay: get(3),
                    damping: get(4),
                    phase: get(5),
                }),
                SourceFunction::Exp => FunctionSpec::Exp(ExpSpec {
                    initial: v0,
                    pulsed: v1,
                    rise_delay: get(2),
                    rise_tau: get(3),
                    fall_delay: get(4),
                    fall_tau: get(5),
                }),
                SourceFunction::Sffm => FunctionSpec::Sffm(SffmSpec {
                    offset: v0,
                    amplitude: v1,
                    carrier: get(2),
                    index: get(3),
                    signal: get(4),
                    delay: get(5),
                    signal_phase: get(6),
                    carrier_phase: get(7),
                }),
                SourceFunction::Am => FunctionSpec::Am(AmSpec {
                    offset: v0,
                    carrier_amplitude: v1,
                    modulation_amplitude: get(2),
                    modulation_frequency: get(3),
                    carrier_frequency: get(4),
                    delay: get(5),
                    modulation_phase: get(6),
                    carrier_phase: get(7),
                }),
            };
            let waveform = Waveform::FunctionDefaults(spec);
            waveform.validate().map_err(|e| waveform_error(at, &e))?;
            Ok(waveform)
        }
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
                count: optional(&p.count, "pulse np").transpose()?,
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
