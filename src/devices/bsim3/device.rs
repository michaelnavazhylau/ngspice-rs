//! BSIM3v3.3 device: the DC load of `b3ld.c` wired into the MNA solver.
//!
//! Charges, NQS, AC, noise, distortion, sensitivity and pole-zero are not
//! ported, so those analyses fail explicitly instead of loading zero. The
//! ACM geometry model and RSH/NRD/NRS internal nodes are refused; every
//! accepted instance therefore has drain/source primes equal to its external
//! nodes, as C's default geometry does.
use std::cell::RefCell;

use super::{
    ModelParameters, Value, charges, dc, defaults::ModelDefaults, setup::InstanceSetup,
    temperature::TemperatureModel,
};
use crate::devices::models::{ModelContext, ResolvedModel};
use crate::devices::nonlinear::{JunctionPoint, junction_companion};
use crate::devices::state::{DeviceState, IterationPhase};
use crate::devices::traits::{Device, StampContext};
use crate::netlist::ast::DeviceInstance;
use crate::primitives::{NodeId, NodeKind, NodeTable, Real, SpiceError, SpiceResult};

/// State slots. The first block is `b3ld.c`'s DC state; the charge block
/// follows C's `bsim3def.h` order, with each integrated charge immediately
/// followed by its derivative slot (C `BSIM3qg`/`BSIM3cqg`, which
/// `NIintegrate` writes as `qcap+1`).
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
    /// `CKTstate0[BSIM3qb]` and its derivative `cqb`.
    pub(super) const QB: usize = 19;
    pub(super) const CQB: usize = 20;
    /// `CKTstate0[BSIM3qg]` and `cqg`.
    pub(super) const QG: usize = 21;
    pub(super) const CQG: usize = 22;
    /// `CKTstate0[BSIM3qd]` and `cqd`.
    pub(super) const QD: usize = 23;
    pub(super) const CQD: usize = 24;
    /// `CKTstate0[BSIM3qbs]`.
    pub(super) const QBS: usize = 25;
    /// `CKTstate0[BSIM3qbd]`.
    pub(super) const QBD: usize = 26;
    /// C `here->BSIM3qinv`, the inversion charge the noise model reads.
    pub(super) const QINV: usize = 27;
    pub(super) const COUNT: usize = 28;
}

/// `b3ld.c`'s `gc*` equivalent charge conductances (`ag0` times the
/// capacitance derivatives) for one mode, named as in C.
#[derive(Debug, Clone, Copy, Default)]
#[allow(clippy::struct_field_names)] // C's own gc*xb names.
struct ChargeConductances {
    gcggb: Real,
    gcgdb: Real,
    gcgsb: Real,
    gcdgb: Real,
    gcddb: Real,
    gcdsb: Real,
    gcsgb: Real,
    gcsdb: Real,
    gcssb: Real,
    gcbgb: Real,
    gcbdb: Real,
    gcbsb: Real,
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
        if context.mode.is_ac() {
            return Err(SpiceError::not_yet_ported(
                format!("{}: BSIM3 small-signal load", self.name),
                "src/spicelib/devices/bsim3/b3acld.c",
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
        let bias = dc::Bias { vbs, vgs, vds };
        let e = self.evaluate_at(&cached, bias, context.gmin)?;
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
        let ConductanceParts {
            gm_eff, fwd, rev, ..
        } = conductance_parts(&e);
        let (cdreq, ceqbs, ceqbd) = if e.mode >= 0 {
            (
                pol * (e.ids - e.gds * vds - e.gm * vgs - e.gmbs * vbs),
                0.,
                -pol * (e.isub - e.gbds * vds - e.gbgs * vgs - e.gbbs * vbs),
            )
        } else {
            let vgd = vgs - vds;
            (
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
        let ConductanceParts {
            gmbs_eff,
            gbdpdp,
            gbdpg,
            gbdpb,
            gbdpsp,
            gbbdp,
            gbbsp,
            gbspg,
            gbspsp,
            gbspb,
            gbspdp,
            ..
        } = conductance_parts(&e);
        // b3ld.c's charge block. C computes it when the analysis needs charge
        // storage (`MODEDCTRANCURVE | MODEAC | MODETRAN | MODEINITSMSIG`, or
        // `MODETRANOP` with `MODEUIC`) and a plain `.op` here needs it too, so
        // asks and a later `.tran` start from C's values. A `capMod` or NQS
        // mode the port has not ported keeps working for DC-only physics and
        // refuses only where charges are required.
        let integration = context.integration;
        let transient = integration.is_some();
        let cap_mod = cached.defaults.get("capmod").unwrap_or(3.).round() as i32;
        let charge_ready = cap_mod == 3 && cached.setup.nqs_mod == 0 && cached.setup.acnqs_mod == 0;
        let charge = if transient || charge_ready {
            Some(
                charges::ChargeModel {
                    model: &cached.defaults,
                    temperature: &cached.temperature_model,
                    setup: &cached.setup,
                }
                .evaluate(&e, bias)?,
            )
        } else {
            None
        };
        let vgb = vgs - vbs;
        let (ceqq, gc) = match charge {
            Some(charge) => stamp_charge(pol, context, charge, e.mode, vgb, vbd, vbs)?,
            None => {
                for slot in [
                    slot::QB,
                    slot::CQB,
                    slot::QG,
                    slot::CQG,
                    slot::QD,
                    slot::CQD,
                    slot::QBS,
                    slot::QBD,
                    slot::QINV,
                ] {
                    context.states.set(slot, 0.)?;
                }
                ([0.; 3], ChargeConductances::default())
            }
        };

        // C `b3ld.c`'s RHS and Y terms. The series conductances connect the
        // external drain/source to their primes and are zero when the primes
        // were not created.
        let gdr = cached.setup.drain_conductance;
        let gsr = cached.setup.source_conductance;
        context.stamp_rhs(g, -m * ceqq[0])?;
        context.stamp_rhs(b, -m * (ceqbs + ceqbd + ceqq[1]))?;
        context.stamp_rhs(dp, m * (ceqbd - cdreq - ceqq[2]))?;
        context.stamp_rhs(sp, m * (cdreq + ceqbs + ceqq[0] + ceqq[1] + ceqq[2]))?;
        context.stamp_fixed([
            (d, d, m * gdr),
            (d, dp, -m * gdr),
            (dp, d, -m * gdr),
            (dp, dp, m * (gdr + e.gds + e.gbd + rev + gbdpdp + gc.gcddb)),
            (dp, g, m * (gm_eff + gbdpg + gc.gcdgb)),
            (
                dp,
                b,
                -m * (e.gbd - gmbs_eff - gbdpb + gc.gcdgb + gc.gcddb + gc.gcdsb),
            ),
            (dp, sp, -m * (e.gds + fwd - gbdpsp - gc.gcdsb)),
            (s, s, m * gsr),
            (s, sp, -m * gsr),
            (sp, s, -m * gsr),
            (sp, sp, m * (gsr + e.gds + e.gbs + fwd + gbspsp + gc.gcssb)),
            (sp, g, m * (-gm_eff + gbspg + gc.gcsgb)),
            (
                sp,
                b,
                -m * (e.gbs + gmbs_eff - gbspb + gc.gcsgb + gc.gcsdb + gc.gcssb),
            ),
            (sp, dp, -m * (e.gds + rev - gbspdp - gc.gcsdb)),
            (
                b,
                b,
                m * (e.gbd + e.gbs - e.gbbs - gc.gcbgb - gc.gcbdb - gc.gcbsb),
            ),
            (b, g, m * (-e.gbgs + gc.gcbgb)),
            (b, dp, m * (-e.gbd + gbbdp + gc.gcbdb)),
            (b, sp, m * (-e.gbs + gbbsp + gc.gcbsb)),
            // C's gate row's charge conductances (`b3ld.c` line900). Its
            // source row's `gcs*` terms are already above; stamping them
            // twice would double the source's charge conductances.
            (g, g, m * gc.gcggb),
            (g, b, -m * (gc.gcggb + gc.gcgdb + gc.gcgsb)),
            (g, dp, m * gc.gcgdb),
            (g, sp, m * gc.gcgsb),
        ])?;
        Ok(())
    }

    /// `b3ask.c`'s bias-dependent values at the solution, times the
    /// multiplicity: the DC quantities, then the `capMod=3` charges and
    /// capacitance derivatives the charge block leaves on the instance.
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
        let dp = self.primes[0].unwrap_or(d);
        let sp = self.primes[1].unwrap_or(s);
        let bias = dc::Bias {
            vbs: pol * (voltage(b) - voltage(sp)),
            vgs: pol * (voltage(g) - voltage(sp)),
            vds: pol * (voltage(dp) - voltage(sp)),
        };
        let e = self.evaluate_at(&cached, bias, context.gmin)?;
        let m = cached.setup.multiplier;
        if let Some(value) = e.ask(keyword) {
            return Ok(Some(value * m));
        }
        let lower = keyword.to_ascii_lowercase();
        if !CHARGE_ASKS.contains(&lower.as_str()) {
            return Ok(None);
        }
        let c = charges::ChargeModel {
            model: &cached.defaults,
            temperature: &cached.temperature_model,
            setup: &cached.setup,
        }
        .evaluate(&e, bias)?;
        let overlap = c.with_overlap(e.mode, bias.vgs - bias.vbs);
        let value = match lower.as_str() {
            "qg" => overlap.qg,
            "qb" => overlap.qb,
            "qd" => overlap.qd,
            "cgg" => c.cggb,
            "cgd" => c.cgdb,
            "cgs" => c.cgsb,
            "cdg" => c.cdgb,
            "cdd" => c.cddb,
            "cds" => c.cdsb,
            "cbg" => c.cbgb,
            "cbd" => c.cbdb,
            "cbs" => c.cbsb,
            "capbd" => c.capbd,
            "capbs" => c.capbs,
            _ => return Ok(None),
        };
        Ok(Some(value * m))
    }

    /// `b3acld.c`: the DC conductances in `A` and the `j omega` capacitance
    /// terms in `E`, both at the operating point's bias.
    fn assemble_small_signal(
        &self,
        context: &mut crate::devices::linear::LinearContext<'_>,
        bias: &crate::maths::Vector,
    ) -> SpiceResult<()> {
        let model = context.model_context;
        let cached = self.ensure(model.temperature, model.nominal_temperature)?;
        let unknowns = context.unknowns;
        let voltage = |node: NodeId| {
            unknowns
                .node_row(node)
                .and_then(|row| bias.get(row))
                .unwrap_or(0.)
        };
        let pol = self.polarity;
        let [d, g, s, b] = [0, 1, 2, 3].map(|k| self.terminals[k]);
        let dp = self.primes[0].unwrap_or(d);
        let sp = self.primes[1].unwrap_or(s);
        let point = dc::Bias {
            vbs: pol * (voltage(b) - voltage(sp)),
            vgs: pol * (voltage(g) - voltage(sp)),
            vds: pol * (voltage(dp) - voltage(sp)),
        };
        let e = self.evaluate_at(&cached, point, model.gmin)?;
        let c = charges::ChargeModel {
            model: &cached.defaults,
            temperature: &cached.temperature_model,
            setup: &cached.setup,
        }
        .evaluate(&e, point)?;
        let m = cached.setup.multiplier;
        let gdr = cached.setup.drain_conductance;
        let gsr = cached.setup.source_conductance;
        let cap = mode_capacitances(&c, e.mode);
        let (cgdo, cgso, cgbo) = (c.cgdo, c.cgso, c.cgbo);
        let (capbd, capbs) = (c.capbd, c.capbs);

        // b3acld.c's real terms, identical to `b3ld.c`'s conductance load.
        let ConductanceParts {
            gm_eff,
            gmbs_eff,
            fwd,
            rev,
            gbdpdp,
            gbdpg,
            gbdpb,
            gbdpsp,
            gbbdp,
            gbbsp,
            gbspg,
            gbspsp,
            gbspb,
            gbspdp,
        } = conductance_parts(&e);
        let real = [
            (d, d, gdr),
            (d, dp, -gdr),
            (dp, d, -gdr),
            (dp, dp, gdr + e.gds + e.gbd + rev + gbdpdp),
            (dp, g, gm_eff + gbdpg),
            (dp, b, -(e.gbd - gmbs_eff - gbdpb)),
            (dp, sp, -(e.gds + fwd - gbdpsp)),
            (s, s, gsr),
            (s, sp, -gsr),
            (sp, s, -gsr),
            (sp, sp, gsr + e.gds + e.gbs + fwd + gbspsp),
            (sp, g, -gm_eff + gbspg),
            (sp, b, -(e.gbs + gmbs_eff - gbspb)),
            (sp, dp, -(e.gds + rev - gbspdp)),
            (b, b, e.gbd + e.gbs - e.gbbs),
            (b, g, -e.gbgs),
            (b, dp, -e.gbd + gbbdp),
            (b, sp, -e.gbs + gbbsp),
        ];
        for (row, col, value) in real {
            if let (Some(r), Some(k)) = (unknowns.node_row(row), unknowns.node_row(col)) {
                context.system.a.add(r, k, m * value)?;
            }
        }

        // b3acld.c's `j omega` terms: C's `xc*` are `cap * omega`, and the
        // complex assembly applies the `omega` factor (see
        // `LinearContext::scaled_dynamic_sum`). The lone `gdsi`/`Gmbsi` terms
        // are zero without AC NQS, which is refused at evaluation.
        let xcdgb = cap.cdgb - cgdo;
        let xcddb = cap.cddb + capbd + cgdo;
        let xcdsb = cap.cdsb;
        let xcsgb = -(cap.cggb + cap.cbgb + cap.cdgb + cgso);
        let xcsdb = -(cap.cgdb + cap.cbdb + cap.cddb);
        let xcssb = capbs + cgso - (cap.cgsb + cap.cbsb + cap.cdsb);
        let xcggb = cap.cggb + cgdo + cgso + cgbo;
        let xcgdb = cap.cgdb - cgdo;
        let xcgsb = cap.cgsb - cgso;
        let xcbgb = cap.cbgb - cgbo;
        let xcbdb = cap.cbdb - capbd;
        let xcbsb = cap.cbsb - capbs;
        let dynamic: [(NodeId, NodeId, &[Real]); 16] = [
            (g, g, &[xcggb]),
            (b, b, &[-(xcbgb + xcbdb + xcbsb)]),
            (dp, dp, &[xcddb]),
            (sp, sp, &[xcssb]),
            (g, b, &[-(xcggb + xcgdb + xcgsb)]),
            (g, dp, &[xcgdb]),
            (g, sp, &[xcgsb]),
            (b, g, &[xcbgb]),
            (b, dp, &[xcbdb]),
            (b, sp, &[xcbsb]),
            (dp, g, &[xcdgb]),
            (dp, b, &[-(xcdgb + xcddb + xcdsb)]),
            (dp, sp, &[xcdsb]),
            (sp, g, &[xcsgb]),
            (sp, b, &[-(xcsgb + xcsdb + xcssb)]),
            (sp, dp, &[xcsdb]),
        ];
        for (row, col, terms) in dynamic {
            context.scaled_dynamic_sum(row, col, terms, m)?;
        }
        Ok(())
    }

    /// C `b3pzld.c` equals `b3acld.c` with `j omega` replaced by `s`: the
    /// same `A + s E` pencil.
    fn assemble_pole_zero(
        &self,
        context: &mut crate::devices::linear::LinearContext<'_>,
        bias: &crate::maths::Vector,
    ) -> SpiceResult<()> {
        self.assemble_small_signal(context, bias)
    }

    /// `b3noi.c`: the drain/source series-resistance thermal sources, the
    /// channel thermal source selected by `noiMod` and the flicker source.
    /// C computes `qinv` in the charge block first, so the noise model needs
    /// the `capMod=3` charge evaluation; other modes refuse rather than
    /// substituting a density.
    fn noise(
        &self,
        context: &crate::devices::noise::NoiseContext<'_>,
    ) -> SpiceResult<crate::devices::noise::DeviceNoise> {
        use crate::devices::noise::{DeviceNoise, FlickerLaw, NoiseFamily, NoiseKind, NoiseSource};
        let model = context.model_context;
        let cached = self.ensure(model.temperature, model.nominal_temperature)?;
        let pol = self.polarity;
        let [d, g, s, b] = [0, 1, 2, 3].map(|k| self.terminals[k]);
        let dp = self.primes[0].unwrap_or(d);
        let sp = self.primes[1].unwrap_or(s);
        let voltage = |node: NodeId| context.voltage(node);
        let bias = dc::Bias {
            vbs: pol * (voltage(b) - voltage(sp)),
            vgs: pol * (voltage(g) - voltage(sp)),
            vds: pol * (voltage(dp) - voltage(sp)),
        };
        let e = self.evaluate_at(&cached, bias, model.gmin)?;
        let charge = charges::ChargeModel {
            model: &cached.defaults,
            temperature: &cached.temperature_model,
            setup: &cached.setup,
        }
        .evaluate(&e, bias)?;
        let densities = super::noise::NoiseModel {
            model: &cached.defaults,
            setup: &cached.setup,
        }
        .densities(&e, &charge, context.circuit_kelvin())?;
        let thermal = |conductance: Real| NoiseKind::Thermal {
            conductance,
            temperature: context.circuit_kelvin(),
        };
        let sources = vec![
            NoiseSource::new(".rd", [dp, d], thermal(densities.drain_conductance)),
            NoiseSource::new(".rs", [sp, s], thermal(densities.source_conductance)),
            NoiseSource::new(".id", [dp, sp], thermal(densities.channel)),
            NoiseSource::new(
                ".1overf",
                [dp, sp],
                NoiseKind::Flicker {
                    numerator: densities.flicker_numerator,
                    law: FlickerLaw::Channel {
                        exponent: Some(densities.flicker_exponent),
                        factors: densities.flicker_factors,
                    },
                },
            ),
        ];
        Ok(DeviceNoise::Sources {
            family: NoiseFamily::Bsim3,
            model: Some(self.model_name.clone()),
            total: true,
            sources,
        })
    }
}

/// `b3ask.c` charge/C-V keywords the charge block leaves on the instance.
/// `qbs`/`qbd` and `cqg`/`cqb`/`cqd` are not in C's instance parameter table,
/// so they are not askable even though `b3ask.c` handles them.
const CHARGE_ASKS: [&str; 14] = [
    "qg", "qb", "qd", "cgg", "cgd", "cgs", "cdg", "cdd", "cds", "cbg", "cbd", "cbs", "capbd",
    "capbs",
];

/// C's mode-selected capacitance derivatives (`b3ld.c`/`b3acld.c`): the
/// inverse mode mirrors gate/drain and derives the drain row from the
/// node-charge sum.
#[derive(Debug, Clone, Copy)]
struct CapDerivatives {
    cggb: Real,
    cgdb: Real,
    cgsb: Real,
    cdgb: Real,
    cddb: Real,
    cdsb: Real,
    cbgb: Real,
    cbdb: Real,
    cbsb: Real,
}

/// `b3acld.c`'s mode branch over the intrinsic capacitance derivatives.
fn mode_capacitances(c: &charges::ChargeEvaluation, mode: i8) -> CapDerivatives {
    if mode >= 0 {
        CapDerivatives {
            cggb: c.cggb,
            cgdb: c.cgdb,
            cgsb: c.cgsb,
            cdgb: c.cdgb,
            cddb: c.cddb,
            cdsb: c.cdsb,
            cbgb: c.cbgb,
            cbdb: c.cbdb,
            cbsb: c.cbsb,
        }
    } else {
        CapDerivatives {
            cggb: c.cggb,
            cgdb: c.cgsb,
            cgsb: c.cgdb,
            cdgb: -(c.cdgb + c.cggb + c.cbgb),
            cddb: -(c.cdsb + c.cgsb + c.cbsb),
            cdsb: -(c.cddb + c.cgdb + c.cbdb),
            cbgb: c.cbgb,
            cbdb: c.cbsb,
            cbsb: c.cbdb,
        }
    }
}

/// `b3ld.c` line900 / `b3acld.c`'s mode-selected conductances and substrate
/// current derivatives, shared so the DC load and the small-signal load cannot
/// diverge.
#[derive(Debug, Clone, Copy)]
struct ConductanceParts {
    gm_eff: Real,
    gmbs_eff: Real,
    fwd: Real,
    rev: Real,
    gbdpdp: Real,
    gbdpg: Real,
    gbdpb: Real,
    gbdpsp: Real,
    gbbdp: Real,
    gbbsp: Real,
    gbspg: Real,
    gbspsp: Real,
    gbspb: Real,
    gbspdp: Real,
}

fn conductance_parts(e: &dc::DcEvaluation) -> ConductanceParts {
    if e.mode >= 0 {
        ConductanceParts {
            gm_eff: e.gm,
            gmbs_eff: e.gmbs,
            fwd: e.gm + e.gmbs,
            rev: 0.,
            gbdpdp: e.gbds,
            gbdpg: e.gbgs,
            gbdpb: e.gbbs,
            gbdpsp: -(e.gbgs + e.gbds + e.gbbs),
            gbbdp: -e.gbds,
            gbbsp: e.gbds + e.gbgs + e.gbbs,
            gbspg: 0.,
            gbspsp: 0.,
            gbspb: 0.,
            gbspdp: 0.,
        }
    } else {
        ConductanceParts {
            gm_eff: -e.gm,
            gmbs_eff: -e.gmbs,
            fwd: 0.,
            rev: e.gm + e.gmbs,
            gbdpdp: 0.,
            gbdpg: 0.,
            gbdpb: 0.,
            gbdpsp: 0.,
            gbbdp: e.gbds + e.gbgs + e.gbbs,
            gbbsp: -e.gbds,
            gbspg: e.gbgs,
            gbspsp: e.gbds,
            gbspb: e.gbbs,
            gbspdp: -(e.gbgs + e.gbds + e.gbbs),
        }
    }
}

/// `b3ld.c`'s charge state, `NIintegrate` companions and `gc*` equivalent
/// conductances at `c`. Returns the gate/bulk/drain equivalent charge currents
/// C's `line860` computes.
fn stamp_charge(
    polarity: Real,
    context: &mut StampContext<'_>,
    c: charges::ChargeEvaluation,
    mode: i8,
    vgb: Real,
    vbd: Real,
    vbs: Real,
) -> SpiceResult<([Real; 3], ChargeConductances)> {
    let overlap = c.with_overlap(mode, vgb);
    for (slot, value) in [
        (slot::QB, overlap.qb),
        (slot::QG, overlap.qg),
        (slot::QD, overlap.qd),
        (slot::QBS, c.qbs),
        (slot::QBD, c.qbd),
        (slot::QINV, c.qinv),
    ] {
        context.states.set(slot, value)?;
    }
    // C's two mode branches: the negative branch mirrors gate/drain and
    // derives the drain row from the node-charge sum.
    let CapDerivatives {
        cggb,
        cgsb,
        cgdb,
        cbgb,
        cbsb,
        cbdb,
        cdgb,
        cdsb,
        cddb,
    } = mode_capacitances(&c, mode);
    let ag0 = context
        .integration
        .map_or(0., |coefficients| coefficients.ag()[0]);
    let (cgdo, cgso, cgbo, capbd, capbs) = (c.cgdo, c.cgso, c.cgbo, c.capbd, c.capbs);
    let gc = if mode >= 0 {
        ChargeConductances {
            gcggb: (cggb + cgdo + cgso + cgbo) * ag0,
            gcgdb: (cgdb - cgdo) * ag0,
            gcgsb: (cgsb - cgso) * ag0,
            gcdgb: (cdgb - cgdo) * ag0,
            gcddb: (cddb + capbd + cgdo) * ag0,
            gcdsb: cdsb * ag0,
            gcsgb: -(cggb + cbgb + cdgb + cgso) * ag0,
            gcsdb: -(cgdb + cbdb + cddb) * ag0,
            gcssb: (capbs + cgso - (cgsb + cbsb + cdsb)) * ag0,
            gcbgb: (cbgb - cgbo) * ag0,
            gcbdb: (cbdb - capbd) * ag0,
            gcbsb: (cbsb - capbs) * ag0,
        }
    } else {
        ChargeConductances {
            gcggb: (cggb + cgdo + cgso + cgbo) * ag0,
            gcgdb: (cgsb - cgdo) * ag0,
            gcgsb: (cgdb - cgso) * ag0,
            gcdgb: -(cggb + cbgb + cdgb + cgdo) * ag0,
            gcddb: (capbd + cgdo - (cgsb + cbsb + cdsb)) * ag0,
            gcdsb: -(cgdb + cbdb + cddb) * ag0,
            gcsgb: (cdgb - cgso) * ag0,
            gcsdb: cdsb * ag0,
            gcssb: (cddb + capbs + cgso) * ag0,
            gcbgb: (cbgb - cgbo) * ag0,
            gcbdb: (cbsb - capbd) * ag0,
            gcbsb: (cbdb - capbs) * ag0,
        }
    };
    // `NIintegrate` for qb, qg and qd (`cap` is zero, so C discards geq).
    let (cqg, cqb, cqd) = if let Some(coefficients) = context.integration {
        let mut derivatives = [0.; 3];
        for (index, (slot, charge)) in [
            (slot::QB, overlap.qb),
            (slot::QG, overlap.qg),
            (slot::QD, overlap.qd),
        ]
        .into_iter()
        .enumerate()
        {
            let (_, derivative) = junction_companion(
                context,
                coefficients,
                JunctionPoint {
                    current: 0.,
                    conductance: 0.,
                    charge,
                    capacitance: 0.,
                },
                slot,
            )?;
            derivatives[index] = derivative;
        }
        (derivatives[1], derivatives[0], derivatives[2])
    } else {
        for slot in [slot::CQB, slot::CQG, slot::CQD] {
            context.states.set(slot, 0.)?;
        }
        (0., 0., 0.)
    };
    let mut ceqq = [
        cqg - gc.gcggb * vgb + gc.gcgdb * vbd + gc.gcgsb * vbs,
        cqb - gc.gcbgb * vgb + gc.gcbdb * vbd + gc.gcbsb * vbs,
        cqd - gc.gcdgb * vgb + gc.gcddb * vbd + gc.gcdsb * vbs,
    ];
    // `b3ld.c` negates the charge equivalents for negative polarity.
    if polarity < 0. {
        ceqq = ceqq.map(|value| -value);
    }
    Ok((ceqq, gc))
}
