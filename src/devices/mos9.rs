//! MOS level 9 (`mos9`): ngspice's modified level 3 on the shared
//! [`crate::devices::mos`] shell.
//!
//! C's `mos9/` is a copy of `mos3/` (Alan Gillespie's level 9). Read as
//! behaviour, the two differ only in:
//!
//! - `mos9.c` (`MOS9mPTable`) and `mos9mpar.c` (`MOS9mParam`): no `NLEV` and
//!   no `GDSNOI` setters, which `mos3/` gained later; every other model
//!   setter, default (`mos9set.c`) and alias is level 3's, including the
//!   table-only `XD`, `ALPHA` and `INPUT_DELTA` that `MOS9mParam` refuses;
//! - `mos9temp.c` (`MOS9temp`): the effective length `L - 2 LD + XL` and
//!   width `W - 2 WD + XW` must be at least `1e-6` m (`E_PARMVAL`), not
//!   merely positive; the temperature/process derivation is otherwise
//!   `mos3temp.c`'s;
//! - `mos9load.c` (`MOS9load`): the `moseq3` block of `mos3load.c` without
//!   its `argb != 0` guard in the velocity-saturated channel-length
//!   modulation (see the divergences below);
//! - `mos9noi.c` (`MOS9noise`): the pre-NLEV generators, i.e. the `spice3`
//!   compatibility branch of `mos3noi.c` unconditionally: channel thermal
//!   noise `8/3 k T |gm|`, flicker `KF m |Id/m|^AF / (f Weff Leff Cox^2)`
//!   with `Weff = W - 2 WD`, `Leff = L - 2 LD`, and thermal generators at the
//!   circuit temperature (`NevalSrc`), not the instance `TEMP`/`DTEMP`;
//! - `mos9par.c` does not multiply W/L/AD/AS/PD/PS by `.options scale`,
//!   which the port's options layer rejects for every device anyway.
//!
//! `mos9acld.c`, `mos9pzld.c`, `mos9trun.c` and `mos9set.c`'s internal
//! nodes (`internal#drain`/`internal#source`) equal their level-3
//! counterparts, so the shell's small-signal, pole-zero, truncation and
//! node-naming code serves both levels; [`Level9`] delegates the card,
//! temperature and drain current to [`crate::devices::mos3`].
//!
//! # Deliberate divergences
//!
//! - Those of level 3 (explicit errors where C warns and continues, `TOX =
//!   0`, `.options badmos3` refused by the options layer).
//! - With VMAX > 0, NSUB given (`alpha != 0`) and KAPPA = 0, `mos9load.c`
//!   evaluates `argc / (argb + argb)` and `arga / argb` at `argb = 0` beyond
//!   `vdsat`, so C's `gds`/`gm`/`gmbs` there are NaN. The port uses level
//!   3's guard (`dldvd = dldem = 0`), which is the exact derivative: with
//!   KAPPA = 0 the channel-length reduction is identically zero. The current
//!   itself is the same.
//! - `.disto` (`mos9dset.c`/`mos9dist.c`) and `.sens` (`mos9sld.c`) are not
//!   ported and fail explicitly.

use crate::devices::ModelContext;
use crate::devices::ResolvedModel;
use crate::devices::mos::{
    CELSIUS_TO_KELVIN, Common, DrainCurrent, Geometry, MosLevel, Mosfet, Operating,
};
use crate::devices::mos3::{Level3, MODEL_WITHOUT_NOISE_SELECTORS, Params};
use crate::devices::noise::{DeviceNoise, NoiseContext, NoiseFamily, NoiseKind};
use crate::devices::schema::ScalarSchema;
use crate::primitives::{Real, SpiceError, SpiceResult};

/// `mos9temp.c`: the smallest effective channel length and width (m).
const MINIMUM_EFFECTIVE_SIZE: Real = 1e-6;

/// The level-9 MOSFET: the shared shell around [`Level9`].
pub(super) type Mos9 = Mosfet<Level9>;

/// Validated level-9 model card: a level-3 card without the NLEV/GDSNOI
/// setters.
#[derive(Debug, Clone, Copy)]
pub(super) struct Level9(Level3);

impl MosLevel for Level9 {
    type Params = Params;
    const LEVEL: u8 = 9;
    const LABEL: &'static str = "MOS9";
    const PRIME_SUFFIXES: [&'static str; 2] = ["internal#drain", "internal#source"];

    /// `mos9mpar.c`/`mos9set.c`: the level-3 setters and defaults, minus
    /// NLEV and GDSNOI, which are refused as unknown setters. The validated
    /// card then carries level 3's `NLEV = 2` default, which selects
    /// `mos9noi.c`'s `2/3 |gm|` channel noise in the shell.
    fn from_card(resolved: &ResolvedModel<'_>) -> SpiceResult<Self> {
        resolved.parameters(&ScalarSchema {
            parameters: MODEL_WITHOUT_NOISE_SELECTORS,
        })?;
        Level3::card(resolved, Self::LABEL).map(Self)
    }

    fn common(&self) -> &Common {
        self.0.common()
    }

    /// `mos9temp.c` refuses (`E_PARMVAL`) an effective length or width below
    /// `1e-6` m.
    fn check_geometry(&self, name: &str, geometry: &Geometry) -> SpiceResult<()> {
        let (length, width) = self.0.effective(geometry);
        if !(length >= MINIMUM_EFFECTIVE_SIZE && length.is_finite()) {
            return Err(SpiceError::circuit(format!(
                "{name}: MOS9 effective channel length L - 2*LD + XL must be at least 1e-6 m \
                 (mos9temp.c), got {length:e}"
            )));
        }
        if !(width >= MINIMUM_EFFECTIVE_SIZE && width.is_finite()) {
            return Err(SpiceError::circuit(format!(
                "{name}: MOS9 effective channel width W - 2*WD + XW must be at least 1e-6 m \
                 (mos9temp.c), got {width:e}"
            )));
        }
        Ok(())
    }

    /// `mos9temp.c` and the start of `mos9load.c`, identical to level 3's.
    fn operating(
        &self,
        name: &str,
        geometry: &Geometry,
        context: &ModelContext,
    ) -> SpiceResult<Operating<Params>> {
        self.0.operating_as(Self::LABEL, name, geometry, context)
    }

    /// `mos9load.c`'s `moseq3` block: level 3's (see the module
    /// documentation for the `argb = 0` guard).
    fn drain_current(
        &self,
        op: &Operating<Params>,
        vgs: Real,
        vds: Real,
        vbs: Real,
    ) -> DrainCurrent {
        self.0.drain_current(op, vgs, vds, vbs)
    }

    /// `mos9noi.c`: the shell's generators with `NLEV = 2` channel noise,
    /// the `spice3` flicker law and every thermal generator at the circuit
    /// temperature (C `NevalSrc`).
    fn noise(device: &Mos9, context: &NoiseContext<'_>) -> SpiceResult<DeviceNoise> {
        let model_context = ModelContext {
            spice3_noise: true,
            ..*context.model_context
        };
        let shape = device
            .model
            .0
            .noise_shape(&device.geometry, NoiseFamily::Mos9);
        let mut noise = device.classic_noise(
            &NoiseContext {
                model_context: &model_context,
                ..*context
            },
            shape,
        )?;
        let circuit = context.model_context.temperature + CELSIUS_TO_KELVIN;
        if let DeviceNoise::Sources { sources, .. } = &mut noise {
            for source in sources {
                if let NoiseKind::Thermal { temperature, .. } = &mut source.kind {
                    *temperature = circuit;
                }
            }
        }
        Ok(noise)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_is_level_three_without_the_noise_selectors() {
        let names: Vec<_> = MODEL_WITHOUT_NOISE_SELECTORS
            .iter()
            .map(|p| p.name)
            .collect();
        assert!(!names.contains(&"nlev") && !names.contains(&"gdsnoi"));
        assert_eq!(
            names.len() + 2,
            crate::devices::mos3::MODEL.len(),
            "{names:?}"
        );
        for name in ["kf", "af", "kappa", "xl", "xw", "delvt0", "uo", "tox"] {
            assert!(names.contains(&name), "{name}");
        }
    }
}
