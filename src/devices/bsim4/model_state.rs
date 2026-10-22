//! Circuit-local `b4set.c`/`b4temp.c` model and geometry ownership.
//!
//! One atomic elaboration is a fresh C-process reference domain. In particular,
//! `b4check.c`'s last-name buffer is not shared between independent Circuits.
use std::cell::RefCell;
use std::rc::Rc;

use super::device::Prepared;
use super::fields::{Instance, Model};
use super::host::node;
use super::{apply, set, temp};
use crate::devices::models::{ModelContext, ModelFamily, ModelResolver};
use crate::devices::schema::temperature_kelvin;
use crate::devices::sweep::InstanceOverride;
use crate::netlist::ast::DeviceInstance;
use crate::primitives::{Real, SourceLoc, SpiceError, SpiceResult};

/// One instance's `.dc @inst[param]` values: the C field, its given flag
/// and the value `DCTsetInstParam` stores.
pub(super) type Swept = Vec<(&'static str, Option<&'static str>, Real)>;

/// Every instance's swept values for one `BSIM4temp` pass, by member in
/// ascending order. C's `DCTsetInstParam` writes one instance and then runs
/// `BSIM4temp` over every model and instance, so a sweep of one instance
/// changes what the others see (fresh size knots without the check's
/// `pParam` resets, the function-local `Lnew`): a pass is keyed by all of
/// them, not by the loaded instance alone.
pub(super) type Overrides = Vec<(usize, Swept)>;

/// One instance's handle on its batch's shared model state.
#[derive(Clone)]
pub(crate) struct Binding {
    owner: Rc<RefCell<ModelState>>,
    index: usize,
    pub(super) created: Vec<(i32, &'static str)>,
}

struct Member {
    name: String,
    location: SourceLoc,
    /// The circuit's device ordinal, once the instance is committed;
    /// `.dc @inst[param]` overrides name instances by it.
    device: Option<usize>,
    instance: Instance,
    warnings: Vec<String>,
}

struct Group {
    name: String,
    model: Model,
    members: Vec<usize>,
    warnings: Vec<String>,
}

struct Pass {
    temperature: Real,
    nominal: Real,
    swept: Overrides,
    prepared: Vec<Rc<Prepared>>,
}

struct ModelState {
    groups: Vec<Group>,
    members: Vec<Member>,
    last_name: String,
    passes: Vec<Pass>,
}

/// Resolve actual scoped/bin-selected model identities before making any nodes.
/// Models are prepended on first reference; instances are prepended per model
/// (`GENnewModel`/`GENnewInstance`), not ordered by model declarations.
pub(crate) fn bind_batch(
    instances: &[DeviceInstance],
    models: &ModelResolver<'_>,
    context: &ModelContext,
    existing: bool,
) -> SpiceResult<Vec<Option<Binding>>> {
    let mut state = ModelState {
        groups: Vec::new(),
        members: Vec::new(),
        last_name: String::new(),
        passes: Vec::new(),
    };
    let mut slots = Vec::new();
    for (slot, card) in instances
        .iter()
        .enumerate()
        .filter(|(_, i)| i.designator == 'm')
    {
        let Some(resolved) = models.resolve(card)? else {
            continue;
        };
        if !matches!(resolved.family(), ModelFamily::Nmos | ModelFamily::Pmos)
            || !matches!(resolved.levels().selector, 14 | 54)
        {
            continue;
        }
        if existing {
            return Err(SpiceError::not_yet_ported(
                "incremental BSIM4 batches after BSIM4 setup; add all BSIM4 instances atomically",
                "bsim4/b4check.c BSIM4checkModel; bsim4/b4temp.c BSIM4temp",
            ));
        }
        context.validate(&card.location)?;
        if card.nodes.len() != 4 {
            return Err(SpiceError::parse(
                card.location.clone(),
                "BSIM4 needs four terminals: drain, gate, source, bulk",
            ));
        }
        let params = super::model_parameters(&resolved)?;
        let name = &resolved.card().name;
        let group = match state.groups.iter().position(|g| g.name == *name) {
            Some(index) => index,
            None => {
                let mut model = apply::model(&params)?;
                let mut warnings = Vec::new();
                set::setup_model(
                    &mut model,
                    temperature_kelvin(context.nominal_temperature, &card.location)?,
                    &mut warnings,
                )?;
                state.groups.push(Group {
                    name: name.clone(),
                    model,
                    members: Vec::new(),
                    warnings,
                });
                state.groups.len() - 1
            }
        };
        let mut instance = apply::instance(&params.instance_parameters(card, 1.)?)?;
        instance.dNode = node::D;
        instance.gNodeExt = node::G_EXT;
        instance.sNode = node::S;
        instance.bNode = node::B;
        let index = state.members.len();
        state.members.push(Member {
            name: card.name.clone(),
            location: card.location.clone(),
            device: None,
            instance,
            warnings: Vec::new(),
        });
        state.groups[group].members.push(index);
        slots.push((slot, index));
    }
    let mut created = vec![Vec::new(); state.members.len()];
    // BSIM4setup runs every model's instance loop before BSIM4temp begins.
    for group in state.groups.iter().rev() {
        for &index in group.members.iter().rev() {
            let member = &mut state.members[index];
            created[index] = set::setup_instance(
                &group.model,
                &mut member.instance,
                context.noise_analysis,
                &mut member.warnings,
            )?;
        }
    }
    if !state.members.is_empty() {
        state.ensure(context.temperature, context.nominal_temperature, Vec::new())?;
    }
    let owner = Rc::new(RefCell::new(state));
    let mut bindings = vec![None; instances.len()];
    for (slot, index) in slots {
        bindings[slot] = Some(Binding {
            owner: Rc::clone(&owner),
            index,
            created: std::mem::take(&mut created[index]),
        });
    }
    Ok(bindings)
}

/// The C field and given flag `DCTsetInstParam` writes for `parameter`,
/// with `value`.
///
/// # Errors
/// A parameter that is not a settable BSIM4 instance field.
pub(super) fn swept_entry(
    parameter: &str,
    value: Real,
) -> SpiceResult<(&'static str, Option<&'static str>, Real)> {
    let p = super::descriptor(super::parameters::INSTANCE, parameter)
        .filter(|p| p.c_field.is_some())
        .ok_or_else(|| {
            SpiceError::circuit(format!(
                "BSIM4 instance parameter {parameter} cannot be swept"
            ))
        })?;
    Ok((p.c_field.unwrap_or_default(), p.c_given, value))
}

impl Binding {
    /// Records the circuit's device ordinal for this instance.
    ///
    /// # Errors
    /// Reentrant access to the shared model state.
    pub(crate) fn attach(&self, device: usize) -> SpiceResult<()> {
        self.borrow_mut()?.members[self.index].device = Some(device);
        Ok(())
    }

    /// The overrides of the pass this instance loads under `context`: its
    /// own swept values (`own`, a `.dc @inst[param]` stand-in's), else any
    /// the context names for it, and the context's overrides of every other
    /// instance sharing this state.
    ///
    /// # Errors
    /// Reentrant access, or an override naming a field BSIM4 cannot set.
    pub(super) fn overrides(
        &self,
        own: &Swept,
        context: &[Option<InstanceOverride>],
    ) -> SpiceResult<Overrides> {
        let owner = self
            .owner
            .try_borrow()
            .map_err(|_| SpiceError::circuit("reentrant BSIM4 model access"))?;
        let mut overrides = Vec::new();
        for (index, member) in owner.members.iter().enumerate() {
            let swept = if index == self.index && !own.is_empty() {
                own.clone()
            } else if let Some(device) = member.device {
                context
                    .iter()
                    .flatten()
                    .filter(|o| o.device() == device)
                    .map(|o| swept_entry(o.parameter(), o.value()))
                    .collect::<SpiceResult<Swept>>()?
            } else {
                Vec::new()
            };
            if !swept.is_empty() {
                overrides.push((index, swept));
            }
        }
        Ok(overrides)
    }

    /// This instance's setup after the `BSIM4temp` pass at these
    /// temperatures with `overrides` applied, run once per distinct pass.
    ///
    /// # Errors
    /// Reentrant access, or the pass's setup failure.
    pub(super) fn ensure(
        &self,
        temperature: Real,
        nominal: Real,
        overrides: Overrides,
    ) -> SpiceResult<Rc<Prepared>> {
        let mut owner = self.borrow_mut()?;
        let pass = owner.ensure(temperature, nominal, overrides)?;
        Ok(Rc::clone(&owner.passes[pass].prepared[self.index]))
    }

    fn borrow_mut(&self) -> SpiceResult<std::cell::RefMut<'_, ModelState>> {
        self.owner
            .try_borrow_mut()
            .map_err(|_| SpiceError::circuit("reentrant BSIM4 model access"))
    }
}

impl ModelState {
    fn ensure(&mut self, temperature: Real, nominal: Real, swept: Overrides) -> SpiceResult<usize> {
        if let Some(index) = self
            .passes
            .iter()
            .position(|p| p.temperature == temperature && p.nominal == nominal && p.swept == swept)
        {
            return Ok(index);
        }
        let initial = self.passes.first();
        let mut prepared: Vec<Option<Rc<Prepared>>> = vec![None; self.members.len()];
        let mut last_name = self.last_name.clone();
        for group in self.groups.iter().rev() {
            let mut model = initial.map_or_else(
                || group.model.clone(),
                |p| p.prepared[group.members[0]].model.clone(),
            );
            let mut model_warnings = if initial.is_none() {
                group.warnings.clone()
            } else {
                Vec::new()
            };
            let kelvin = temperature_kelvin(temperature, &self.members[group.members[0]].location)?;
            let locals = temp::temperature_model(&mut model, kelvin, &mut model_warnings);
            let mut sizes = temp::SizeCache::default();
            let mut instances = Vec::new();
            for &index in group.members.iter().rev() {
                let member = &self.members[index];
                let mut instance = initial.map_or_else(
                    || member.instance.clone(),
                    |p| p.prepared[index].instance.clone(),
                );
                if let Some((_, values)) = swept.iter().find(|(target, _)| *target == index) {
                    for &(field, given, value) in values {
                        *instance.real_mut(field).ok_or_else(|| {
                            SpiceError::circuit(format!("BSIM4 swept field {field} has no member"))
                        })? = value;
                        if let Some(flag) = given.and_then(|g| instance.flag_mut(g)) {
                            *flag = true;
                        }
                    }
                }
                let mut warnings = if initial.is_none() {
                    member.warnings.clone()
                } else {
                    Vec::new()
                };
                if instances.is_empty() {
                    warnings.splice(0..0, model_warnings.drain(..));
                }
                // b4check.c's static char modname[BSIZE_SP], not a name set.
                let check = group.name.len() >= 512
                    || !last_name.as_bytes().starts_with(group.name.as_bytes());
                if check && group.name.len() < 512 {
                    last_name.clone_from(&group.name);
                }
                let size = temp::temperature_instance_shared(
                    &mut model,
                    &mut instance,
                    &locals,
                    check,
                    &group.name,
                    &member.name,
                    &mut warnings,
                    &mut sizes,
                )
                .map_err(|e| match e {
                    SpiceError::NotYetPorted { .. } => e,
                    other => SpiceError::circuit(format!("{}: {other}", member.name)),
                })?;
                instances.push((index, instance, size, warnings));
            }
            // Every load sees the final shared model/geometry, but each instance
            // keeps the values its own temperature-loop iteration wrote.
            for (index, instance, size, warnings) in instances {
                prepared[index] = Some(Rc::new(Prepared {
                    temperature,
                    nominal,
                    overrides: swept.clone(),
                    model: model.clone(),
                    size: sizes.find(&instance).cloned().unwrap_or(size),
                    instance,
                    warnings,
                }));
            }
        }
        let prepared = prepared
            .into_iter()
            .map(|p| p.ok_or_else(|| SpiceError::circuit("missing BSIM4 instance setup")))
            .collect::<SpiceResult<_>>()?;
        if self.passes.len() >= 8 {
            self.passes.remove(1);
        }
        self.passes.push(Pass {
            temperature,
            nominal,
            swept,
            prepared,
        });
        self.last_name = last_name;
        Ok(self.passes.len() - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netlist::{Parser, source::parse_deck_text};
    use std::path::Path;

    #[test]
    fn shared_model_bindings_retain_one_check_and_geometry_resets() {
        let text = "state\nm1 d g 0 0 nn l=1u w=2u\nm2 d g 0 0 nn l=2u w=2u\nm3 d g 0 0 nn l=1u w=2u\n.model nn nmos level=54 a1=0.8 a2=1.2 rdsw=-10 prwg=-1 ckappas=0.01 ckappad=0.01\n.end\n";
        let deck = Parser::new()
            .parse_deck(&parse_deck_text(Path::new("state.cir"), text))
            .unwrap();
        let models = ModelResolver::new(&deck.models).unwrap();
        let bindings = bind_batch(&deck.devices, &models, &ModelContext::default(), false).unwrap();
        let p: Vec<_> = bindings
            .iter()
            .map(|b| b.as_ref().unwrap().ensure(27., 27., Vec::new()).unwrap())
            .collect();
        assert_eq!(p[0].size.a2, 1.);
        assert!(p[1].size.a2 > 1.);
        assert_eq!(p[2].size.a2, 1.);
        assert_eq!(
            p.iter()
                .flat_map(|p| &p.warnings)
                .filter(|w| w.contains("Checking parameters"))
                .count(),
            1
        );
        assert!(p[2].warnings.iter().any(|w| w.contains("A2 = 1.2")));
        assert!(p[0].warnings.is_empty());
        let warnings: Vec<_> = p[2].warnings.iter().map(String::as_str).collect();
        assert_eq!(
            warnings,
            vec![
                "\nChecking parameters for BSIM 4.8 model nn",
                "Warning: ckappas = 0.01 is too small.",
                "Warning: ckappad = 0.01 is too small.",
                "Warning: A2 = 1.2 is larger than 1. A2 is set to 1 and A1 is set to 0.",
                "Warning: Prwg = -1 is negative. Set to zero.",
                "Warning: Rdsw = -10 is negative. Set to zero.",
            ]
        );
        // The next temperature pass builds fresh knots and skips nn's check.
        // Cached initial knots retain their resets; later ones must not inherit them.
        for binding in bindings.iter().flatten() {
            let later = binding.ensure(47., 27., Vec::new()).unwrap();
            assert!(later.size.a2 > 1.);
            assert!(
                !later
                    .warnings
                    .iter()
                    .any(|w| w.contains("Checking parameters"))
            );
        }
        let original = bindings[0]
            .as_ref()
            .unwrap()
            .ensure(27., 27., Vec::new())
            .unwrap();
        assert!(Rc::ptr_eq(&original, &p[0]));
        assert_eq!(original.size.a2, 1.);
    }
    #[test]
    fn shared_model_resets_and_cached_geometry_keep_c_mutation_order() {
        let text = "state\nm1 d g 0 0 nn l=1u w=2u\nm2 d g 0 0 nn l=2u w=2u\nm3 d g 0 0 nn l=1u w=2u\n.model nn nmos level=54 rgatemod=1 rshg=1 xgl=0.2u cgdo=-0.01\n.end\n";
        let deck = Parser::new()
            .parse_deck(&parse_deck_text(Path::new("state.cir"), text))
            .unwrap();
        let models = ModelResolver::new(&deck.models).unwrap();
        let bindings = bind_batch(&deck.devices, &models, &ModelContext::default(), false).unwrap();
        let p: Vec<_> = bindings
            .iter()
            .map(|b| b.as_ref().unwrap().ensure(27., 27., Vec::new()).unwrap())
            .collect();
        for prepared in &p {
            assert_eq!(prepared.model.cgdo, 0.);
        }
        // pParam for m1/m3 was made before the model-level cgdo reset;
        // the different geometry was made afterwards and sees that reset.
        assert!(p[0].size.cgdo < 0.);
        assert_eq!(p[0].size.cgdo, p[2].size.cgdo);
        assert!(p[1].size.cgdo > 0.);
        // m1 reuses m3's knot but C's function-local Lnew was last assigned
        // by m2. Recomputing m1's size block would incorrectly give 1.2 S.
        assert!((p[0].instance.grgeltd - 2.7).abs() < 1e-14);
        assert!((p[2].instance.grgeltd - 1.2).abs() < 1e-14);
    }
}
