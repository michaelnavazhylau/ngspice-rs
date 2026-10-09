//! Linear controlled sources: E (VCVS), F (CCCS), G (VCCS) and H (CCVS).
//!
//! C references: `src/spicelib/devices/vcvs/` (`vcvsset.c`, `vcvsload.c`,
//! `vcvspar.c`), `cccs/`, `vccs/`, `ccvs/`, and the controlling-branch lookup
//! `src/spicelib/analysis/cktfbran.c` (`CKTfndBranch`).
//!
//! # Equations and signs
//!
//! Output current is positive from the first terminal (`n+`) through the
//! source to the second (`n-`), as for independent sources. Controlling
//! voltages are `v(nc+) - v(nc-)`; controlling currents are the branch
//! current of the named source, positive from its first terminal through it
//! to its second (so a source delivering power has a negative current).
//!
//! | Device | Branch row | Stamps (`A x = b`, ground rows/columns dropped) |
//! | --- | --- | --- |
//! | E `n+ n- nc+ nc- mu` | `k` | `A[n+][k] += 1`, `A[n-][k] -= 1`, `A[k][n+] += 1`, `A[k][n-] -= 1`, `A[k][nc+] -= mu`, `A[k][nc-] += mu` |
//! | G `n+ n- nc+ nc- gm` | none | `A[n+][nc+] += gm`, `A[n+][nc-] -= gm`, `A[n-][nc+] -= gm`, `A[n-][nc-] += gm` |
//! | F `n+ n- vc beta` | none | `A[n+][kc] += beta`, `A[n-][kc] -= beta` |
//! | H `n+ n- vc r` | `k` | E's output stamps, then `A[k][kc] -= r` |
//!
//! so E enforces `v(n+) - v(n-) = mu (v(nc+) - v(nc-))`, H enforces
//! `v(n+) - v(n-) = r i(vc)`, G draws `gm (v(nc+) - v(nc-))` out of `n+` into
//! `n-` through itself, and F does the same with `beta i(vc)`. All four are
//! state-independent: the same real stamps serve OP, DC sweeps, AC (no
//! frequency dependence, `*acld` equals `*load`), companion transients and the
//! immutable `E x' + A x = b(t)` assembly used by the diffsol BDF backend. They
//! add nothing to `E` or `b`.
//!
//! # Setter semantics
//!
//! Parameters are applied in stored order: `gain` sets the coefficient and, for
//! G/F, multiplies it by `m` only if `m` was given earlier (`VCCSparam`,
//! `CCCSparam`); `m` alone never rescales an earlier gain. E/H whose output
//! terminals coincide are rejected like `VCVSsetup`/`CCVSsetup` ("shorted").

use crate::maths::SparseMatrix;
use crate::netlist::ast::{DeviceInstance, ParameterKind};
use crate::primitives::{
    NodeId, NodeTable, Real, SourceLoc, SpiceError, SpiceResult, parse_spice_number,
};

use crate::devices::linear::LinearContext;
use crate::devices::traits::{ControlReference, Device, MnaUnknowns, StampContext};

/// Which controlled source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlledKind {
    /// E: voltage-controlled voltage source, gain in V/V.
    Vcvs,
    /// G: voltage-controlled current source, transconductance in S.
    Vccs,
    /// F: current-controlled current source, gain in A/A.
    Cccs,
    /// H: current-controlled voltage source, transresistance in ohms.
    Ccvs,
}

impl ControlledKind {
    /// The designator letter.
    #[must_use]
    pub const fn designator(self) -> char {
        match self {
            Self::Vcvs => 'e',
            Self::Vccs => 'g',
            Self::Cccs => 'f',
            Self::Ccvs => 'h',
        }
    }

    /// The kind for a designator letter.
    #[must_use]
    pub const fn from_designator(designator: char) -> Option<Self> {
        match designator {
            'e' => Some(Self::Vcvs),
            'g' => Some(Self::Vccs),
            'f' => Some(Self::Cccs),
            'h' => Some(Self::Ccvs),
            _ => None,
        }
    }

    /// True for the voltage-controlled E/G, which have controlling nodes.
    #[must_use]
    pub const fn voltage_controlled(self) -> bool {
        matches!(self, Self::Vcvs | Self::Vccs)
    }

    /// True for E/H, which add a branch-current unknown.
    #[must_use]
    pub const fn has_branch(self) -> bool {
        matches!(self, Self::Vcvs | Self::Ccvs)
    }

    const fn c_name(self) -> &'static str {
        match self {
            Self::Vcvs => "VCVS",
            Self::Vccs => "VCCS",
            Self::Cccs => "CCCS",
            Self::Ccvs => "CCVS",
        }
    }
}

/// A linear controlled source. See the [module documentation](self).
#[derive(Debug, Clone, PartialEq)]
pub struct ControlledSource {
    name: String,
    kind: ControlledKind,
    /// `[n+, n-]`, then `[nc+, nc-]` for E/G.
    terminals: Vec<NodeId>,
    gain: Real,
    control: Option<ControlReference>,
    /// The G/F instance `m`, if given (`VCCSmGiven`/`CCCSmGiven`): a gain set
    /// later through `@g1[gain]` is multiplied by it.
    multiplier: Option<Real>,
}

impl ControlledSource {
    /// A voltage-controlled source (E or G).
    ///
    /// # Errors
    /// A non-finite gain, a current-controlled `kind`, or an E whose output
    /// terminals coincide.
    pub fn voltage_controlled(
        name: impl Into<String>,
        kind: ControlledKind,
        output: [NodeId; 2],
        control: [NodeId; 2],
        gain: Real,
    ) -> SpiceResult<Self> {
        if !kind.voltage_controlled() {
            return Err(SpiceError::circuit(format!(
                "{kind:?} is not voltage controlled"
            )));
        }
        Self::checked(
            name.into(),
            kind,
            vec![output[0], output[1], control[0], control[1]],
            gain,
            None,
        )
    }

    /// A current-controlled source (F or H) sensing `control`'s branch current.
    ///
    /// # Errors
    /// A non-finite gain, a voltage-controlled `kind`, or an H whose output
    /// terminals coincide.
    pub fn current_controlled(
        name: impl Into<String>,
        kind: ControlledKind,
        output: [NodeId; 2],
        control: ControlReference,
        gain: Real,
    ) -> SpiceResult<Self> {
        if kind.voltage_controlled() {
            return Err(SpiceError::circuit(format!(
                "{kind:?} is not current controlled"
            )));
        }
        Self::checked(
            name.into(),
            kind,
            vec![output[0], output[1]],
            gain,
            Some(control),
        )
    }

    fn checked(
        name: String,
        kind: ControlledKind,
        terminals: Vec<NodeId>,
        gain: Real,
        control: Option<ControlReference>,
    ) -> SpiceResult<Self> {
        if !gain.is_finite() {
            return Err(SpiceError::circuit(format!("{name}: non-finite gain")));
        }
        if kind.has_branch() && terminals[0] == terminals[1] {
            return Err(SpiceError::Unsupported {
                feature: format!("instance {name} is a shorted {}", kind.c_name()),
                location: None,
            });
        }
        Ok(Self {
            name,
            kind,
            terminals,
            gain,
            control,
            multiplier: None,
        })
    }

    /// A copy stamping the coefficient `coefficient` (C's stored
    /// `VCVScoeff`, ...), for a `.sens` load.
    ///
    /// # Errors
    /// A nonfinite coefficient.
    pub(crate) fn with_coefficient(&self, coefficient: Real) -> SpiceResult<Self> {
        if !coefficient.is_finite() {
            return Err(SpiceError::circuit(format!(
                "{}: nonfinite perturbed gain {coefficient}",
                self.name
            )));
        }
        Ok(Self {
            gain: coefficient,
            ..self.clone()
        })
    }

    /// Which controlled source this is.
    #[must_use]
    pub const fn kind(&self) -> ControlledKind {
        self.kind
    }

    /// The effective gain after `m` scaling.
    #[must_use]
    pub const fn gain(&self) -> Real {
        self.gain
    }

    /// The sensed source of an F/H, `None` for E/G.
    #[must_use]
    pub const fn control(&self) -> Option<&ControlReference> {
        self.control.as_ref()
    }

    /// Stamps the static operator into `matrix` (shared by every analysis).
    fn stamp_into(
        &self,
        matrix: &mut SparseMatrix,
        unknowns: &MnaUnknowns,
        branch: Option<usize>,
        controls: &[usize],
    ) -> SpiceResult<()> {
        let row = |node: NodeId| unknowns.node_row(node);
        let (p, n) = (row(self.terminals[0]), row(self.terminals[1]));
        let missing_branch = || SpiceError::circuit(format!("{}: missing branch row", self.name));
        let control_branch = || {
            controls.first().copied().ok_or_else(|| {
                SpiceError::circuit(format!(
                    "{}: controlling source branch is not bound",
                    self.name
                ))
            })
        };
        let mut add = |r: Option<usize>, c: Option<usize>, value: Real| match (r, c) {
            (Some(r), Some(c)) => matrix.add(r, c, value),
            _ => Ok(()),
        };
        match self.kind {
            ControlledKind::Vccs => {
                let (cp, cn) = (row(self.terminals[2]), row(self.terminals[3]));
                add(p, cp, self.gain)?;
                add(p, cn, -self.gain)?;
                add(n, cp, -self.gain)?;
                add(n, cn, self.gain)?;
            }
            ControlledKind::Cccs => {
                let kc = Some(control_branch()?);
                add(p, kc, self.gain)?;
                add(n, kc, -self.gain)?;
            }
            ControlledKind::Vcvs | ControlledKind::Ccvs => {
                let k = Some(branch.ok_or_else(missing_branch)?);
                add(p, k, 1.)?;
                add(n, k, -1.)?;
                add(k, p, 1.)?;
                add(k, n, -1.)?;
                if self.kind == ControlledKind::Vcvs {
                    let (cp, cn) = (row(self.terminals[2]), row(self.terminals[3]));
                    add(k, cp, -self.gain)?;
                    add(k, cn, self.gain)?;
                } else {
                    add(k, Some(control_branch()?), -self.gain)?;
                }
            }
        }
        Ok(())
    }
}

impl Device for ControlledSource {
    /// Noiseless: C gives this device no noise routine (`DEVnoise = NULL`,
    /// `src/spicelib/devices/{vcvs,vccs,cccs,ccvs}/*init.c`).
    fn noise(
        &self,
        _context: &crate::devices::noise::NoiseContext<'_>,
    ) -> crate::primitives::SpiceResult<crate::devices::noise::DeviceNoise> {
        Ok(crate::devices::noise::DeviceNoise::Noiseless)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn designator(&self) -> char {
        self.kind.designator()
    }

    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }

    fn branch_currents(&self) -> usize {
        usize::from(self.kind.has_branch())
    }

    fn controlling_sources(&self) -> &[ControlReference] {
        self.control.as_slice()
    }

    /// E/H currents can control other F/H sources (`VCVSfindBr`, `CCVSfindBr`).
    fn findable_branch(&self) -> Option<usize> {
        self.kind.has_branch().then_some(0)
    }

    /// The same real stamps in every analysis mode (`*load.c` equals `*acld.c`).
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        let branch = (!context.branches.is_empty()).then_some(context.branches.start);
        self.stamp_into(context.matrix, context.unknowns, branch, context.controls)
    }

    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        self.stamp_into(
            &mut context.system.a,
            context.unknowns,
            context.branch,
            context.controls,
        )
    }

    /// Pole-zero load: C `vcvspzld.c`, `vccspzld.c`, `cccspzld.c`, `ccvspzld.c` equals the AC load with `s` for `j omega`.
    fn assemble_pole_zero(
        &self,
        context: &mut crate::devices::linear::LinearContext<'_>,
        bias: &crate::maths::Vector,
    ) -> crate::primitives::SpiceResult<()> {
        self.assemble_small_signal(context, bias)
    }

    /// `.sens`: C's coefficient and `m` records
    /// ([`crate::devices::sensitivity`]).
    fn sensitivity(
        &self,
        _context: &crate::devices::models::ModelContext,
    ) -> SpiceResult<Box<dyn crate::devices::sensitivity::DeviceSensitivity + '_>> {
        Ok(Box::new(
            crate::devices::sensitivity::ControlledSensitivity::new(
                self,
                self.gain,
                self.multiplier,
            ),
        ))
    }

    /// `gain` of E/F/G/H (`vcvs.c`, `cccs.c`, `vccs.c`, `ccvs.c`).
    fn instance_parameter(&self, keyword: &str) -> Option<&'static str> {
        keyword.eq_ignore_ascii_case("gain").then_some("gain")
    }

    /// `VCVSparam`/`CCVSparam` store the swept gain as is; `VCCSparam` and
    /// `CCCSparam` multiply it by the instance `m` when one was given anywhere
    /// on the card (`VCCSmGiven`), which is what `dctrcurv.c` sees.
    fn with_instance_parameter(
        &self,
        parameter: &str,
        value: Real,
        _context: &crate::devices::models::ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if parameter != "gain" {
            return Err(SpiceError::circuit(format!(
                "{}: controlled-source parameter {parameter} cannot be swept",
                self.name
            )));
        }
        let gain = value * self.multiplier.unwrap_or(1.);
        if !gain.is_finite() {
            return Err(SpiceError::circuit(format!(
                "{}: swept gain {value} is not finite",
                self.name
            )));
        }
        Ok(Box::new(Self {
            gain,
            ..self.clone()
        }))
    }
}

/// Builds an E/F/G/H from its parsed (and literalized) AST instance, binding
/// terminals in `nodes` only on success.
pub(crate) fn instantiate(
    instance: &DeviceInstance,
    nodes: &mut NodeTable,
) -> SpiceResult<Box<dyn Device>> {
    let Some(kind) = ControlledKind::from_designator(instance.designator) else {
        return Err(SpiceError::circuit(format!(
            "{} is not a controlled source",
            instance.name
        )));
    };
    if let Some(form) = instance
        .parameters
        .iter()
        .find(|p| matches!(p.name.as_str(), "poly" | "value" | "table"))
    {
        // The front end rewrites these into B sources/XSPICE instances
        // (crate::netlist::behavioural::lower_nonlinear_sources, run by
        // Circuit::from_netlist), as inpcom.c does before INP2E..INP2H.
        return Err(SpiceError::Unsupported {
            feature: format!(
                "{}: the {} form must be lowered with \
                 crate::netlist::behavioural::lower_nonlinear_sources before instantiation",
                instance.name,
                form.name.to_ascii_uppercase()
            ),
            location: Some(instance.location.clone()),
        });
    }
    let expected = if kind.voltage_controlled() { 4 } else { 2 };
    if instance.model.is_some() || instance.nodes.len() != expected {
        return Err(SpiceError::Unsupported {
            feature: format!(
                "{} needs {expected} nodes and no model in its linear form",
                instance.name
            ),
            location: Some(instance.location.clone()),
        });
    }
    let multiplier_allowed = matches!(kind, ControlledKind::Vccs | ControlledKind::Cccs);
    let mut gain: Option<Real> = None;
    let mut multiplier: Option<Real> = None;
    let mut control: Option<ControlReference> = None;
    for parameter in &instance.parameters {
        match (&parameter.kind, parameter.name.as_str()) {
            (ParameterKind::Instance, "control") if !kind.voltage_controlled() => {
                if control.is_some() {
                    return Err(SpiceError::Unsupported {
                        feature: format!("{}: more than one controlling source", instance.name),
                        location: Some(parameter.location.clone()),
                    });
                }
                control = Some(ControlReference {
                    name: parameter.value.to_ascii_lowercase(),
                    location: Some(parameter.location.clone()),
                });
            }
            (ParameterKind::Scalar, "gain") => {
                let value = literal(parameter.value.as_str(), &parameter.location, "gain")?;
                // VCCSparam/CCCSparam: an already-given m scales the new gain.
                gain = Some(value * multiplier.unwrap_or(1.));
            }
            (ParameterKind::Scalar, "m") if multiplier_allowed => {
                multiplier = Some(literal(parameter.value.as_str(), &parameter.location, "m")?);
            }
            (ParameterKind::Expression(_) | ParameterKind::Textual, _) => {
                return Err(SpiceError::Unsupported {
                    feature: format!(
                        "non-literal {} parameter {}={} (expressions must be literalized first)",
                        instance.name, parameter.name, parameter.value
                    ),
                    location: Some(parameter.location.clone()),
                });
            }
            _ => {
                return Err(SpiceError::Unsupported {
                    feature: format!("{} parameter {}", instance.name, parameter.name),
                    location: Some(parameter.location.clone()),
                });
            }
        }
    }
    let Some(gain) = gain else {
        return Err(SpiceError::Unsupported {
            feature: format!("{} has no gain", instance.name),
            location: Some(instance.location.clone()),
        });
    };
    let mut staged = nodes.clone();
    let mut bind = |name: &String| staged.intern(name);
    let output = [bind(&instance.nodes[0]), bind(&instance.nodes[1])];
    let device = if kind.voltage_controlled() {
        let control = [bind(&instance.nodes[2]), bind(&instance.nodes[3])];
        ControlledSource::voltage_controlled(&instance.name, kind, output, control, gain)
    } else {
        let control = control.ok_or_else(|| SpiceError::Unsupported {
            feature: format!("{} has no controlling source", instance.name),
            location: Some(instance.location.clone()),
        })?;
        ControlledSource::current_controlled(&instance.name, kind, output, control, gain)
    }
    .map_err(|error| match error {
        SpiceError::Unsupported { feature, .. } => SpiceError::Unsupported {
            feature,
            location: Some(instance.location.clone()),
        },
        other => other,
    })?;
    let device = ControlledSource {
        multiplier,
        ..device
    };
    *nodes = staged;
    Ok(Box::new(device))
}

fn literal(text: &str, location: &SourceLoc, what: &str) -> SpiceResult<Real> {
    parse_spice_number(text)
        .filter(|value| value.is_finite())
        .ok_or_else(|| SpiceError::Unsupported {
            feature: format!("non-finite or nonliteral {what}={text}"),
            location: Some(location.clone()),
        })
}
