//! Explicit setters onto C's structs: what `b3mpar.c`/`b3par.c` store,
//! applied in deck order to a zero-initialised model or instance, as
//! `GENmodel`/`GENinstance` allocation and the parser leave them.
use super::fields::{Instance, Model};
use super::{Inputs, ModelParameters, Value};
use crate::primitives::{SpiceError, SpiceResult};

/// Store one applied setter in the struct through its generated accessors.
fn store<T>(
    target: &mut T,
    real: for<'a> fn(&'a mut T, &str) -> Option<&'a mut f64>,
    int: for<'a> fn(&'a mut T, &str) -> Option<&'a mut i32>,
    flag: for<'a> fn(&'a mut T, &str) -> Option<&'a mut bool>,
    field: &str,
    given: Option<&str>,
    value: &Value,
) -> SpiceResult<()> {
    let missing = || {
        SpiceError::circuit(format!(
            "BSIM3 setter destination {field} has no struct member"
        ))
    };
    match value {
        Value::Real(v) => match real(target, field) {
            Some(slot) => *slot = *v,
            None => return Err(missing()),
        },
        Value::Integer(v) => {
            if let Some(slot) = int(target, field) {
                *slot = *v;
            } else if let Some(slot) = real(target, field) {
                *slot = f64::from(*v);
            } else {
                return Err(missing());
            }
        }
        // Flags store C's `iValue` (1) in an int member.
        Value::Flag(v) => match int(target, field) {
            Some(slot) => *slot = i32::from(*v),
            None => return Err(missing()),
        },
        Value::String(_) => return Err(missing()),
    }
    if let Some(given) = given {
        *flag(target, given).ok_or_else(|| {
            SpiceError::circuit(format!("BSIM3 given bit {given} has no struct member"))
        })? = true;
    }
    Ok(())
}

/// The model struct as `b3mpar.c` leaves it, before `BSIM3setup`.
///
/// # Errors
/// A setter whose generated destination is missing (an internal error).
pub(crate) fn model(params: &ModelParameters) -> SpiceResult<Model> {
    // INPmakeMod applies the NMOS/PMOS type before the card's setters.
    let mut m = Model {
        r#type: i32::from(params.polarity),
        typeGiven: true,
        ..Model::default()
    };
    for applied in params.inputs.applied() {
        let p = applied.parameter;
        if let Value::String(text) = &applied.value {
            m.version.clone_from(text);
            m.versionGiven = true;
            continue;
        }
        if matches!(applied.value, Value::Flag(_)) {
            // `nmos`/`pmos`: the effective polarity was computed in order.
            m.r#type = i32::from(params.polarity);
            continue;
        }
        let field = p.c_field.ok_or_else(|| {
            SpiceError::circuit(format!("BSIM3 model setter {} has no field", p.name))
        })?;
        store(
            &mut m,
            Model::real_mut,
            Model::int_mut,
            Model::flag_mut,
            field,
            p.c_given,
            &applied.value,
        )?;
    }
    Ok(m)
}

/// The instance struct as `b3par.c` leaves it, before `BSIM3setup`.
///
/// # Errors
/// A setter whose generated destination is missing (an internal error).
pub(crate) fn instance(inputs: &Inputs) -> SpiceResult<Instance> {
    let mut h = Instance::default();
    for applied in inputs.applied() {
        let p = applied.parameter;
        let field = p.c_field.ok_or_else(|| {
            SpiceError::circuit(format!("BSIM3 instance setter {} has no field", p.name))
        })?;
        store(
            &mut h,
            Instance::real_mut,
            Instance::int_mut,
            Instance::flag_mut,
            field,
            p.c_given,
            &applied.value,
        )?;
    }
    Ok(h)
}
