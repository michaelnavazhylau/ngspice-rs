//! BSIM3v3.3 device: the DC load of `b3ld.c` wired into the MNA solver.
//!
//! Charges, NQS, AC, noise, distortion, sensitivity and pole-zero are not
//! ported, so those analyses fail explicitly instead of loading zero. The
//! ACM geometry model and RSH/NRD/NRS internal nodes are refused; every
//! accepted instance therefore has drain/source primes equal to its external
//! nodes, as C's default geometry does.
use std::cell::RefCell;

use super::{
    ModelParameters, Value, dc, defaults::ModelDefaults, setup::InstanceSetup,
    temperature::TemperatureModel,
};
use crate::devices::models::{ModelContext, ResolvedModel};
use crate::devices::state::{DeviceState, IterationPhase};
use crate::devices::traits::{Device, StampContext};
use crate::netlist::ast::DeviceInstance;
use crate::primitives::{NodeId, NodeKind, NodeTable, Real, SpiceError, SpiceResult};

/// State slots. C's `BSIM3numStates` also covers the charge slots, which are
/// deliberately absent until the charge model is ported.
mod slot {
    pub(super) const VBS: usize = 0;
    pub(super) const VBD: usize = 1;
    pub(super) const VGS: usize = 2;
    pub(super) const VDS: usize = 3;
    pub(super) const VON: usize = 4;
    pub(super) const VDSAT: usize = 5;
    pub(super) const MODE: usize = 6;
    pub(super) const CD: usize = 7;
    pub(super) const CBS: usize = 8;
    pub(super) const CBD: usize = 9;
    pub(super) const GM: usize = 10;
    pub(super) const GDS: usize = 11;
    pub(super) const GMBS: usize = 12;
    pub(super) const GBD: usize = 13;
    pub(super) const GBS: usize = 14;
    pub(super) const GBGS: usize = 15;
    pub(super) const GBDS: usize = 16;
    pub(super) const GBBS: usize = 17;
    pub(super) const CSUB: usize = 18;
    pub(super) const COUNT: usize = 19;
}

/// `devsup.c`'s thermal voltage at 300.15 K (`CONSTvt0` in `main.c`).
const CONST_VT0: Real = 1.380_648_52e-23 * (27.0 + 273.15) / 1.602_176_620_8e-19;

/// How this load treated the supplied voltages (C's start/limiting phases).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Start {
    Conditions,
    Junction,
    HeldOff,
    Limited,
    Exact,
}

struct Cached {
    temperature: Real,
    nominal: Real,
    defaults: ModelDefaults,
    temperature_model: TemperatureModel,
    setup: InstanceSetup,
}

/// One BSIM3 instance. Holds the defaulted model and effective parameters per
/// (temperature, nominal temperature), as C re-runs `BSIM3temp` on changes.
pub struct Bsim3 {
    name: String,
    model_name: String,
    /// External drain/gate/source/bulk, then any internal primes.
    terminals: Vec<NodeId>,
    /// Drain/source prime nodes, present when C's series conductance is nonzero.
    primes: [Option<NodeId>; 2],
    params: ModelParameters,
    instance: DeviceInstance,
    polarity: Real,
    off: bool,
    ic: [Option<Real>; 3],
    cache: RefCell<Option<Cached>>,
}

impl std::fmt::Debug for Bsim3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bsim3")
            .field("name", &self.name)
            .field("model", &self.model_name)
            .field("polarity", &self.polarity)
            .finish_non_exhaustive()
    }
}

impl Bsim3 {
    /// Build an instance, validating everything before interning nodes.
    ///
    /// # Errors
    /// Invalid schema/setup input, ACM geometry, RSH-created internal nodes,
    /// or a nonpositive effective size (`b3check.c`).
    pub(crate) fn instantiate(
        instance: &DeviceInstance,
        nodes: &mut NodeTable,
        model: &ResolvedModel<'_>,
        context: &ModelContext,
    ) -> SpiceResult<Box<dyn Device>> {
        if instance.nodes.len() != 4 {
            return Err(SpiceError::parse(
                instance.location.clone(),
                "BSIM3 needs four terminals: drain, gate, source, bulk",
            ));
        }
        let params = super::model_parameters(model)?;
        let inputs = params.instance_parameters(instance, 1.)?;
        let defaults = params.model_defaults(context)?;
        let temperature_model = defaults.at_temperature(context.temperature, &instance.location)?;
        let setup = InstanceSetup::new(&defaults, &temperature_model, &inputs, &instance.location)?;
        let off = inputs
            .get("off")
            .is_some_and(|applied| matches!(applied.value, Value::Flag(true)));
        let component = |name: &str| -> Option<Real> {
            inputs.get(name).map(|applied| match applied.value {
                Value::Real(value) => value,
                _ => 0.,
            })
        };
        let ic = [component("icvbs"), component("icvgs"), component("icvds")];
        let mut staged = nodes.clone();
        let mut terminals: Vec<NodeId> = instance.nodes.iter().map(|n| staged.intern(n)).collect();
        let mut primes = [None; 2];
        for (index, conductance, suffix) in [
            (0, setup.drain_conductance, "drain"),
            (1, setup.source_conductance, "source"),
        ] {
            if conductance <= 0. {
                continue;
            }
            let name = format!("{}#{suffix}", instance.name);
            if staged.get(&name).is_some() {
                return Err(SpiceError::circuit(format!(
                    "{}: internal node name collision: {name}",
                    instance.name
                )));
            }
            let prime = staged.intern(&name);
            staged.set_kind(prime, NodeKind::Internal);
            terminals.push(prime);
            primes[index] = Some(prime);
        }
        *nodes = staged;
        let device = Self {
            name: instance.name.clone(),
            model_name: model.card().name.clone(),
            terminals,
            primes,
            polarity: Real::from(setup.polarity),
            params,
            instance: instance.clone(),
            off,
            ic,
            cache: RefCell::new(Some(Cached {
                temperature: context.temperature,
                nominal: context.nominal_temperature,
                defaults,
                temperature_model,
                setup,
            })),
        };
        Ok(Box::new(device))
    }

    /// The setup for these temperatures, rebuilt when C would re-run
    /// `BSIM3temp`. A snapshot is returned so no borrow outlives the call.
    fn ensure(&self, temperature: Real, nominal: Real) -> SpiceResult<Cached> {
        let mut cache = self
            .cache
            .try_borrow_mut()
            .map_err(|_| SpiceError::circuit(format!("{}: reentrant BSIM3 load", self.name)))?;
        let matches = cache
            .as_ref()
            .is_some_and(|c| c.temperature == temperature && c.nominal == nominal);
        if !matches {
            let context = ModelContext::new(temperature, nominal);
            let defaults = self.params.model_defaults(&context)?;
            let temperature_model =
                defaults.at_temperature(temperature, &self.instance.location)?;
            let inputs = self.params.instance_parameters(&self.instance, 1.)?;
            let setup = InstanceSetup::new(
                &defaults,
                &temperature_model,
                &inputs,
                &self.instance.location,
            )?;
            *cache = Some(Cached {
                temperature,
                nominal,
                defaults,
                temperature_model,
                setup,
            });
        }
        let cached = cache.as_ref().expect("present");
        Ok(Cached {
            temperature: cached.temperature,
            nominal: cached.nominal,
            defaults: cached.defaults.clone(),
            temperature_model: cached.temperature_model.clone(),
            setup: cached.setup.clone(),
        })
    }

    /// C `b3getic.c`: under `uic`, unset IC components are the terminal
    /// voltages; otherwise they are zero (`b3set.c` defaults).
    fn start_conditions(
        &self,
        conditions: bool,
        voltage: impl Fn(NodeId) -> Real,
    ) -> [Option<Real>; 3] {
        let [d, g, s, b] = [0, 1, 2, 3].map(|k| self.terminals[k]);
        let mut result = self.ic;
        if conditions {
            let filled = [
                voltage(b) - voltage(s),
                voltage(g) - voltage(s),
                voltage(d) - voltage(s),
            ];
            for (value, fallback) in result.iter_mut().zip(filled) {
                if value.is_none() {
                    *value = Some(fallback);
                }
            }
        }
        result
    }

    /// C `b3ld.c`'s start and limiting block.
    fn select(
        cached: &Cached,
        states: &DeviceState<'_>,
        raw: [Real; 3],
        ic: [Option<Real>; 3],
        off: bool,
        polarity: Real,
    ) -> ([Real; 3], Start, bool) {
        let scaled = [0, 1, 2].map(|k| polarity * ic[k].unwrap_or(0.));
        if states.initial_conditions() {
            let v = if off { [0.; 3] } else { scaled };
            return (v, Start::Conditions, true);
        }
        if !states.device_limiting() {
            return (raw, Start::Exact, false);
        }
        match states.phase() {
            IterationPhase::Junction => {
                let v = if off {
                    [0.; 3]
                } else if scaled.iter().all(|v| *v == 0.) {
                    // `b3ld.c`: Vbs=0, Vgs=type*Vth0+0.1, Vds=0.1.
                    [0., polarity * cached.setup.vth0 + 0.1, 0.1]
                } else {
                    scaled
                };
                (v, Start::Junction, true)
            }
            IterationPhase::Fix if off => ([0.; 3], Start::HeldOff, true),
            IterationPhase::Predict => {
                let values = (
                    states.predicted(slot::VBS, 1.),
                    states.predicted(slot::VGS, 1.),
                    states.predicted(slot::VDS, 1.),
                    states.accepted(1, slot::VBS),
                    states.accepted(1, slot::VGS),
                    states.accepted(1, slot::VDS),
                    states.accepted(1, slot::VON),
                );
                let (
                    Some(_vbs),
                    Some(_vgs),
                    Some(_vds),
                    Some(old_vbs),
                    Some(old_vgs),
                    Some(old_vds),
                    Some(von),
                ) = values
                else {
                    return (raw, Start::Exact, false);
                };
                let (v, check) = limit(raw, [old_vbs, old_vgs, old_vds], von, cached);
                (v, Start::Limited, check)
            }
            IterationPhase::Fix | IterationPhase::Float => {
                let previous = |slot: usize| states.iterate(slot).filter(|v| v.is_finite());
                let values = (
                    previous(slot::VBS),
                    previous(slot::VBD),
                    previous(slot::VGS),
                    previous(slot::VDS),
                    previous(slot::VON),
                );
                let (Some(old_vbs), Some(old_vbd), Some(old_vgs), Some(old_vds), Some(von)) =
                    values
                else {
                    return (raw, Start::Exact, false);
                };
                let (v, check) = limit(raw, [old_vbs, old_vgs, old_vds], von, cached);
                let _ = old_vbd;
                (v, Start::Limited, check)
            }
        }
    }

    fn evaluate_at(
        &self,
        cached: &Cached,
        bias: dc::Bias,
        gmin: Real,
    ) -> SpiceResult<dc::DcEvaluation> {
        dc::DcModel {
            model: &cached.defaults,
            temperature: &cached.temperature_model,
            setup: &cached.setup,
        }
        .evaluate(bias, gmin)
    }
}

/// `b3ld.c`'s limiting block over `DEVfetlim`, `DEVlimvds` and `DEVpnjlim`.
fn limit(raw: [Real; 3], old: [Real; 3], von: Real, cached: &Cached) -> ([Real; 3], bool) {
    let [vbs_in, vgs_in, vds_in] = raw;
    let [old_vbs, old_vgs, old_vds] = old;
    let vcrit = cached.temperature_model.get("vcrit").unwrap_or(0.);
    let vgd_raw = vgs_in - vds_in;
    let mut check = false;
    let (vgs, vds);
    if old_vds >= 0. {
        let limited_vgs = fetlim(vgs_in, old_vgs, von);
        let limited_vds = limvds(limited_vgs - vgd_raw, old_vds);
        vgs = limited_vgs;
        vds = limited_vds;
    } else {
        let limited_vgd = fetlim(vgd_raw, old_vgs - old_vds, von);
        let limited_vds = -limvds(-(vgs_in - limited_vgd), -old_vds);
        vds = limited_vds;
        vgs = limited_vgd + limited_vds;
    }
    let vbs = if vds >= 0. {
        let (limited, changed) = pnjlim(vbs_in, old_vbs, CONST_VT0, vcrit);
        check |= changed;
        limited
    } else {
        let vbd_in = vbs_in - vds_in;
        let (limited, changed) = pnjlim(vbd_in, vbs_in - old_vds, CONST_VT0, vcrit);
        check |= changed;
        limited + vds
    };
    ([vbs, vgs, vds], check)
}

/// `devsup.c::DEVfetlim`.
fn fetlim(vnew: Real, vold: Real, vto: Real) -> Real {
    let vtsthi = (2. * (vold - vto)).abs() + 2.;
    let vtstlo = (vold - vto).abs() + 1.;
    let vtox = vto + 3.5;
    let delv = vnew - vold;
    if vold >= vto {
        if vold >= vtox {
            if delv <= 0. {
                if vnew >= vtox {
                    if -delv > vtstlo { vold - vtstlo } else { vnew }
                } else {
                    vnew.max(vto + 2.)
                }
            } else if delv >= vtsthi {
                vold + vtsthi
            } else {
                vnew
            }
        } else if delv <= 0. {
            vnew.max(vto - 0.5)
        } else {
            vnew.min(vto + 4.)
        }
    } else if delv <= 0. {
        if -delv > vtsthi { vold - vtsthi } else { vnew }
    } else {
        let vtemp = vto + 0.5;
        if vnew <= vtemp {
            if delv > vtstlo { vold + vtstlo } else { vnew }
        } else {
            vtemp
        }
    }
}

/// `devsup.c::DEVlimvds`.
fn limvds(vnew: Real, vold: Real) -> Real {
    if vold >= 3.5 {
        if vnew > vold {
            vnew.min(3. * vold + 2.)
        } else if vnew < 3.5 {
            vnew.max(2.)
        } else {
            vnew
        }
    } else if vnew > vold {
        vnew.min(4.)
    } else {
        vnew.max(-0.5)
    }
}

/// `devsup.c::DEVpnjlim`, returning the limited voltage and C's `icheck`.
fn pnjlim(vnew: Real, vold: Real, vt: Real, vcrit: Real) -> (Real, bool) {
    if vnew > vcrit && (vnew - vold).abs() > vt + vt {
        let value = if vold > 0. {
            let arg = (vnew - vold) / vt;
            if arg > 0. {
                vold + vt * (2. + (arg - 2.).ln())
            } else {
                vold - vt * (2. + (2. - arg).ln())
            }
        } else {
            vt * (vnew / vt).ln()
        };
        (value, true)
    } else if vnew < 0. {
        let arg = if vold > 0. {
            -vold - 1.
        } else {
            2. * vold - 1.
        };
        if vnew < arg {
            (arg, true)
        } else {
            (vnew, false)
        }
    } else {
        (vnew, false)
    }
}

impl Device for Bsim3 {
    fn name(&self) -> &str {
        &self.name
    }

    fn model_name(&self) -> Option<&str> {
        Some(&self.model_name)
    }

    fn designator(&self) -> char {
        'm'
    }

    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }

    fn is_nonlinear(&self) -> bool {
        true
    }

    fn has_start_settings(&self) -> bool {
        self.off || self.ic.iter().any(Option::is_some)
    }

    fn state_count(&self) -> usize {
        slot::COUNT
    }

    fn c_device_type(&self) -> crate::devices::setup::CDeviceType {
        crate::devices::setup::CDeviceType::Bsim
    }

    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() || context.integration.is_some() {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "{}: BSIM3 charge and small-signal loads (capMod/NQS)",
                    self.name
                ),
                "src/spicelib/devices/bsim3/b3ld.c (charges), b3acld.c, b3noi.c, b3pzld.c",
            ));
        }
        let cached = self.ensure(context.temperature, context.nominal_temperature)?;
        let [d, g, s, b] = [0, 1, 2, 3].map(|k| self.terminals[k]);
        let dp = self.primes[0].unwrap_or(d);
        let sp = self.primes[1].unwrap_or(s);
        let voltage = |node: NodeId| context.node_voltage(node);
        let pol = self.polarity;
        let raw = [
            pol * (voltage(b) - voltage(sp)),
            pol * (voltage(g) - voltage(sp)),
            pol * (voltage(dp) - voltage(sp)),
        ];
        let ic = self.start_conditions(context.states.initial_conditions(), voltage);
        let ([vbs, vgs, vds], start, check) =
            Self::select(&cached, &context.states, raw, ic, self.off, pol);
        let e = self.evaluate_at(&cached, dc::Bias { vbs, vgs, vds }, context.gmin)?;
        let report = match start {
            Start::Junction => true,
            Start::Limited => check,
            Start::Conditions | Start::Exact | Start::HeldOff => false,
        };
        if report {
            context.states.report_nonconvergence()?;
        }
        for (slot, value) in [
            (slot::VBS, vbs),
            (slot::VBD, vbs - vds),
            (slot::VGS, vgs),
            (slot::VDS, vds),
            (slot::VON, e.von),
            (slot::VDSAT, e.vdsat),
            (slot::MODE, Real::from(e.mode)),
            (slot::CD, e.ids),
            (slot::CBS, e.cbs),
            (slot::CBD, e.cbd),
            (slot::GM, e.gm),
            (slot::GDS, e.gds),
            (slot::GMBS, e.gmbs),
            (slot::GBD, e.gbd),
            (slot::GBS, e.gbs),
            (slot::GBGS, e.gbgs),
            (slot::GBDS, e.gbds),
            (slot::GBBS, e.gbbs),
            (slot::CSUB, e.isub),
        ] {
            context.states.set(slot, value)?;
        }

        // b3ld.c's line900 terms with RD=RS=0 (so the primes are the external
        // nodes) and the DC-only zero charge terms.
        let m = cached.setup.multiplier;
        let vbd = vbs - vds;
        let (gm_eff, gmbs_eff, fwd, rev, cdreq, ceqbs, ceqbd) = if e.mode >= 0 {
            (
                e.gm,
                e.gmbs,
                e.gm + e.gmbs,
                0.,
                pol * (e.ids - e.gds * vds - e.gm * vgs - e.gmbs * vbs),
                0.,
                -pol * (e.isub - e.gbds * vds - e.gbgs * vgs - e.gbbs * vbs),
            )
        } else {
            let vgd = vgs - vds;
            (
                -e.gm,
                -e.gmbs,
                0.,
                e.gm + e.gmbs,
                -pol * (e.ids + e.gds * vds - e.gm * vgd - e.gmbs * vbd),
                -pol * (e.isub + e.gbds * vds - e.gbgs * vgd - e.gbbs * vbd),
                0.,
            )
        };
        let (ceqbs, ceqbd) = if pol > 0. {
            (ceqbs + (e.cbs - e.gbs * vbs), ceqbd + (e.cbd - e.gbd * vbd))
        } else {
            (ceqbs - (e.cbs - e.gbs * vbs), ceqbd - (e.cbd - e.gbd * vbd))
        };
        let (gbdpdp, gbdpg, gbdpb, gbdpsp, gbbdp, gbbsp, gbspg, gbspsp, gbspb, gbspdp) =
            if e.mode >= 0 {
                (
                    e.gbds,
                    e.gbgs,
                    e.gbbs,
                    -(e.gbgs + e.gbds + e.gbbs),
                    -e.gbds,
                    e.gbds + e.gbgs + e.gbbs,
                    0.,
                    0.,
                    0.,
                    0.,
                )
            } else {
                (
                    0.,
                    0.,
                    0.,
                    0.,
                    e.gbds + e.gbgs + e.gbbs,
                    -e.gbds,
                    e.gbgs,
                    e.gbds,
                    e.gbbs,
                    -(e.gbgs + e.gbds + e.gbbs),
                )
            };
        // C `b3ld.c`'s RHS and Y terms. The series conductances connect the
        // external drain/source to their primes and are zero when the primes
        // were not created.
        let gdr = cached.setup.drain_conductance;
        let gsr = cached.setup.source_conductance;
        context.stamp_rhs(b, -m * (ceqbs + ceqbd))?;
        context.stamp_rhs(dp, m * (ceqbd - cdreq))?;
        context.stamp_rhs(sp, m * (cdreq + ceqbs))?;
        context.stamp_fixed([
            (d, d, m * gdr),
            (d, dp, -m * gdr),
            (dp, d, -m * gdr),
            (dp, dp, m * (gdr + e.gds + e.gbd + rev + gbdpdp)),
            (dp, g, m * (gm_eff + gbdpg)),
            (dp, b, -m * (e.gbd - gmbs_eff + gbdpb)),
            (dp, sp, -m * (e.gds + fwd - gbdpsp)),
            (s, s, m * gsr),
            (s, sp, -m * gsr),
            (sp, s, -m * gsr),
            (sp, sp, m * (gsr + e.gds + e.gbs + fwd + gbspsp)),
            (sp, g, m * (-gm_eff + gbspg)),
            (sp, b, -m * (e.gbs + gmbs_eff + gbspb)),
            (sp, dp, -m * (e.gds + rev - gbspdp)),
            (b, b, m * (e.gbd + e.gbs - e.gbbs)),
            (b, g, -m * e.gbgs),
            (b, dp, m * (-e.gbd + gbbdp)),
            (b, sp, m * (-e.gbs + gbbsp)),
        ])?;
        Ok(())
    }

    /// `b3ask.c`'s bias-dependent values at the solution, times the
    /// multiplicity. Charge, NQS and noise asks are not available yet.
    fn observation_operating(
        &self,
        keyword: &str,
        context: &ModelContext,
        voltage: &dyn Fn(NodeId) -> Real,
        _transient: bool,
    ) -> SpiceResult<Option<Real>> {
        let cached = self.ensure(context.temperature, context.nominal_temperature)?;
        let pol = self.polarity;
        let [d, g, s, b] = [0, 1, 2, 3].map(|k| self.terminals[k]);
        let bias = dc::Bias {
            vbs: pol * (voltage(b) - voltage(s)),
            vgs: pol * (voltage(g) - voltage(s)),
            vds: pol * (voltage(d) - voltage(s)),
        };
        let e = self.evaluate_at(&cached, bias, context.gmin)?;
        Ok(e.ask(keyword).map(|value| value * cached.setup.multiplier))
    }

    fn assemble_small_signal(
        &self,
        _context: &mut crate::devices::linear::LinearContext<'_>,
        _bias: &crate::maths::Vector,
    ) -> SpiceResult<()> {
        Err(SpiceError::not_yet_ported(
            format!("{}: BSIM3 small-signal load", self.name),
            "src/spicelib/devices/bsim3/b3acld.c",
        ))
    }

    fn noise(
        &self,
        _context: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        Err(SpiceError::not_yet_ported(
            format!("{}: BSIM3 noise", self.name),
            "src/spicelib/devices/bsim3/b3noi.c",
        ))
    }
}
