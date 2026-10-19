//! Shared input plumbing for the Berkeley short-channel models.
//!
//! BSIM3 and BSIM4 describe their parameters with the same `IFparm` tables and
//! their setters with the same one-destination-per-case form, so the ordered,
//! provenance-preserving projection lives here once. Each device supplies a
//! [`Device`] descriptor: its generated tables, the diagnostic reference, the
//! IC component order and its own model-level real conversion
//! (`*mpar.c` TNOM/doping thresholds).
use crate::devices::schema::{ScalarUnit, finite_literal};
use crate::netlist::ast::{ParameterAssignment, ParameterKind};
use crate::primitives::{Real, SourceLoc, SpiceError, SpiceResult};

/// One device family's generated tables and conversion rules.
///
/// Compared by identity: two `Inputs` are equal when they were built from the
/// same device descriptor and carry the same applied setters.
pub(crate) struct Device {
    /// Diagnostic reference for the C setter file, e.g. `bsim3/b3.c`.
    pub(crate) reference: &'static str,
    /// Generated model table, in C order.
    pub(crate) model: &'static [Parameter],
    /// Generated instance table, in C order.
    pub(crate) instance: &'static [Parameter],
    /// `TABLE:keyword` rows C marks `IF_UNINTERESTING`.
    pub(crate) uninteresting: &'static [&'static str],
    /// The IC vector's component keywords, in C's fallthrough order.
    pub(crate) ic_names: [&'static str; 3],
    /// Model-level real setter conversion (`*mpar.c`).
    pub(crate) model_real: fn(&Parameter, Real, &SourceLoc) -> SpiceResult<Real>,
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device")
            .field("reference", &self.reference)
            .finish_non_exhaustive()
    }
}

impl PartialEq for Device {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

/// A setup message C prints (to stderr or `b3v33check.log`) instead of
/// returning it. Not a substitute for the eventual load diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// True for C's `Fatal_Flag` checks, which stop setup.
    pub fatal: bool,
    /// C's message text, without a trailing newline.
    pub message: String,
    /// Upstream file/function.
    pub c_reference: &'static str,
}

impl Diagnostic {
    /// A warning: C reports it and continues.
    #[must_use]
    pub fn warning(message: impl Into<String>, c_reference: &'static str) -> Self {
        Self {
            fatal: false,
            message: message.into(),
            c_reference,
        }
    }
    /// A fatal check: C reports it and fails setup.
    #[must_use]
    pub fn fatal(message: impl Into<String>, c_reference: &'static str) -> Self {
        Self {
            fatal: true,
            message: message.into(),
            c_reference,
        }
    }
}

/// The input value type declared by the C IFparm table, not a guessed unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    /// IF_REAL, finite in the Rust projection.
    Real,
    /// IF_INTEGER, `floor(value + 0.5)` with checked i32 conversion.
    Integer,
    /// IF_FLAG, a bare keyword (C supplies integer 1).
    Flag,
    /// IF_STRING, preserving the version spelling instead of parsing a number.
    String,
    /// IF_REALVEC, the IC vector expanded into its scalar component setters.
    RealVector,
}

/// C's public interface access, including ask-only rows that are not setters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// IP: settable, not askable.
    Input,
    /// OP: askable, not settable.
    Output,
    /// IOP/IOPR: settable and askable.
    ReadWrite,
}

/// One generated `b3.c` table entry, in C declaration order. Destination names
/// are factual identifiers, not copied C source or executable field access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parameter {
    /// Public keyword, including redundant aliases.
    pub name: &'static str,
    /// First keyword with this ID; aliases share last-set precedence.
    pub canonical: &'static str,
    /// Numeric IFparm identifier from `bsim3def.h`.
    pub id: u16,
    /// C's symbolic identifier, for audits and future setter integration.
    pub c_id: &'static str,
    /// Setter destination; absent for ask-only rows and the aggregate IC vector.
    pub c_field: Option<&'static str>,
    /// C's given-bit destination, absent when the setter writes none.
    pub c_given: Option<&'static str>,
    /// C's input data type.
    pub input_type: InputType,
    /// Read/write capability.
    pub access: Access,
    /// Whether the table macro marks this a redundant alias (IOPR).
    pub redundant: bool,
}

impl Parameter {
    #[allow(clippy::too_many_arguments)] // Generated, reviewable IFparm records.
    pub(crate) const fn new(
        name: &'static str,
        canonical: &'static str,
        id: u16,
        c_id: &'static str,
        c_field: Option<&'static str>,
        c_given: Option<&'static str>,
        input_type: InputType,
        access: Access,
        redundant: bool,
    ) -> Self {
        Self {
            name,
            canonical,
            id,
            c_id,
            c_field,
            c_given,
            input_type,
            access,
            redundant,
        }
    }

    /// Reviewed input units for the unambiguous initial subset. `None` means
    /// not audited yet, **not dimensionless**. In particular mobility and
    /// geometry-binning coefficients need the setup phase's unit conventions.
    #[must_use]
    pub fn input_unit(&self) -> Option<ScalarUnit> {
        use ScalarUnit as U;
        match self.canonical {
            "l" | "w" | "pd" | "ps" | "tox" | "toxm" | "xj" | "xt" | "w0" | "nlx" | "lmin"
            | "lmax" | "wmin" | "wmax" | "lint" | "wint" | "xl" | "xw" => Some(U::Metre),
            "ad" | "as" => Some(U::SquareMetre),
            "icvds" | "icvgs" | "icvbs" | "vth0" | "vbx" | "vbm" | "voff" | "vfb" | "vfbcv" => {
                Some(U::Volt)
            }
            "tnom" => Some(U::Celsius),
            "nsub" | "nch" | "ngate" => Some(U::PerCubicCentimetre),
            "cgso" | "cgdo" | "cgbo" | "cjsw" | "cjswg" => Some(U::FaradPerMetre),
            "cj" => Some(U::FaradPerSquareMetre),
            "js" => Some(U::AmperePerSquareMetre),
            "rsh" => Some(U::Ohm),
            "m" | "nrd" | "nrs" | "mulu0" => Some(U::Dimensionless),
            _ if matches!(self.input_type, InputType::Flag | InputType::Integer) => {
                Some(U::Dimensionless)
            }
            _ => None,
        }
    }
}

/// An explicit input after `b3mpar.c`/`b3par.c` setter conversion, not after
/// setup or temperature processing. The AST retains the original spelling.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Finite scalar; TNOM is stored in Kelvin, as in `BSIM3mParam`.
    Real(Real),
    /// Checked C integer setter value.
    Integer(i32),
    /// Bare IF_FLAG input.
    Flag(bool),
    /// Unchanged bare string spelling.
    String(String),
}

/// One applied setter and its source, including repeated and alias setters.
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedParameter {
    /// Descriptor of the keyword actually used, not just its canonical alias.
    pub parameter: &'static Parameter,
    /// The value stored by the setter.
    pub value: Value,
    /// Source of the original assignment (or IC component).
    pub location: SourceLoc,
}

/// Ordered explicit inputs. No zero/default values are fabricated for unset
/// fields. Aliases compare by C ID; `applied()` retains every duplicate.
#[derive(Debug, Clone, PartialEq)]
pub struct Inputs {
    device: &'static Device,
    table: &'static [Parameter],
    is_model: bool,
    applied: Vec<AppliedParameter>,
}

impl Inputs {
    pub(crate) fn new(device: &'static Device, is_model: bool) -> Self {
        Self {
            device,
            table: if is_model {
                device.model
            } else {
                device.instance
            },
            is_model,
            applied: Vec::new(),
        }
    }
    /// Every applied setter, in application order; IC is expanded in C's
    /// VBS/VGS/VDS fallthrough order. This does not rewrite the source AST.
    #[must_use]
    pub fn applied(&self) -> &[AppliedParameter] {
        &self.applied
    }
    /// Last setter for a keyword or any alias of its C parameter ID.
    /// `None` means ungiven, not a default or a simulated ask value.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&AppliedParameter> {
        let id = descriptor(self.table, name)?.id;
        self.applied
            .iter()
            .rev()
            .find(|value| value.parameter.id == id)
    }

    pub(crate) fn apply(&mut self, p: &ParameterAssignment, scale: Real) -> SpiceResult<()> {
        let descriptor = descriptor(self.table, &p.name)
            .ok_or_else(|| unsupported_setter(p, "unknown setter", self.device.reference))?;
        if descriptor.access == Access::Output {
            return Err(unsupported_setter(
                p,
                "ask-only parameter is not a setter",
                self.device.reference,
            ));
        }
        if descriptor.input_type == InputType::RealVector {
            let ParameterKind::InitialConditions(components) = &p.kind else {
                return Err(unsupported_setter(
                    p,
                    "IC requires a numeric vector",
                    self.device.reference,
                ));
            };
            let names = self.device.ic_names;
            if components.is_empty()
                || components.len() > names.len()
                || components.iter().zip(names).any(|(c, name)| c.name != name)
            {
                return Err(SpiceError::parse(
                    p.location.clone(),
                    "IC expects VDS[,VGS[,VBS]] in order",
                ));
            }
            for component in components.iter().rev() {
                self.apply(
                    &ParameterAssignment {
                        name: component.name.clone(),
                        value: component.value.text.clone(),
                        kind: ParameterKind::Scalar,
                        location: component.value.location.clone(),
                    },
                    scale,
                )?;
            }
            return Ok(());
        }
        let value = match descriptor.input_type {
            InputType::Real => {
                let mut value = finite_literal(p)?;
                if self.is_model {
                    value = (self.device.model_real)(descriptor, value, &p.location)?;
                } else {
                    // b3par.c multiplies left-to-right, not by scale^2:
                    // a small area can remain finite even when scale^2 overflows.
                    value = match descriptor.canonical {
                        "ad" | "as" => value * scale * scale,
                        "l" | "w" | "pd" | "ps" => value * scale,
                        _ => value,
                    };
                }
                if !value.is_finite() {
                    return Err(SpiceError::parse(
                        p.location.clone(),
                        "setter conversion overflow",
                    ));
                }
                Value::Real(value)
            }
            InputType::Integer => {
                let value = (finite_literal(p)? + 0.5).floor();
                if !(Real::from(i32::MIN)..=Real::from(i32::MAX)).contains(&value) {
                    return Err(SpiceError::parse(
                        p.location.clone(),
                        "integer setter overflows i32",
                    ));
                }
                Value::Integer(value as i32)
            }
            InputType::Flag if p.kind == ParameterKind::Flag && p.value.is_empty() => {
                Value::Flag(true)
            }
            InputType::String => Value::String(version_text(p)?.to_owned()),
            _ => {
                return Err(unsupported_setter(
                    p,
                    "setter has the wrong input shape",
                    self.device.reference,
                ));
            }
        };
        self.applied.push(AppliedParameter {
            parameter: descriptor,
            value,
            location: p.location.clone(),
        });
        Ok(())
    }
}

pub(crate) fn descriptor(table: &'static [Parameter], name: &str) -> Option<&'static Parameter> {
    table
        .iter()
        .find(|parameter| parameter.name.eq_ignore_ascii_case(name))
}
fn unsupported_setter(p: &ParameterAssignment, what: &str, reference: &str) -> SpiceError {
    SpiceError::Unsupported {
        feature: format!("{what}: '{}' ({reference})", p.name),
        location: Some(p.location.clone()),
    }
}

pub(crate) fn version_text(p: &ParameterAssignment) -> SpiceResult<&str> {
    if !matches!(p.kind, ParameterKind::Scalar | ParameterKind::Textual)
        || p.value.is_empty()
        || !p
            .value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
    {
        return Err(SpiceError::Unsupported {
            feature: format!(
                "BSIM version must be one bare literal token, not an expression or quoted \
                 string: '{}'",
                p.name
            ),
            location: Some(p.location.clone()),
        });
    }
    if p.kind == ParameterKind::Scalar {
        finite_literal(p)?;
    }
    Ok(&p.value)
}
