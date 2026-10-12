//! C's device setup order and matrix element allocation (`CKTsetup`).
//!
//! ngspice creates every matrix element once, at setup, before any load:
//! `CKTsetup` (`cktsetup.c`) calls `DEVsetup` for each device type in the
//! order of `static_devices[]` (`dev.c`), and each `DEVsetup` walks its model
//! list and every model's instance list, allocating elements with
//! `TSTALLOC`/`SMPmakeElt`. `CKTload` walks the same lists in the same order.
//! Both lists are built by prepending (`CKTmodCrt`, `CKTcrtElt`), and the
//! parser creates a model at its first instance reference (`INPgetMod`) or,
//! for a model-less instance, a per-type default model (`defRmod`,
//! `defVmod`, ...). So within a type, models run from the last created to the
//! first, and each model's instances from the last card to the first.
//!
//! SPARSE numbers rows and columns by their first appearance in those
//! allocations (`Translate`, `spbuild.c`), and Markowitz pivoting breaks ties
//! by that internal numbering, so this order and the full element set
//! (zero-valued elements included) decide the pivot sequence. Load order
//! decides the rounding of every summed element. [`setup_order`] reproduces
//! the traversal; [`Device::setup_matrix`](crate::devices::Device::setup_matrix)
//! reproduces each device's `TSTALLOC` sequence.

use std::ops::Range;

use crate::devices::traits::MnaUnknowns;
use crate::primitives::{NodeId, SpiceError, SpiceResult};

/// The device types of ngspice's `static_devices[]` (`dev.c`), in table
/// order. Only the order matters here; types the port does not implement are
/// listed so later ports slot in at the right place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CDeviceType {
    /// `urc` (uniform RC line; must precede R and C).
    Urc,
    /// `asrc` (B sources and the lowered E/G/F/H VALUE/TABLE/POLY forms).
    Asrc,
    /// `bjt` (Gummel-Poon).
    Bjt,
    /// `bsim1` .. `b3soidd` (BSIM and SOI families, unported).
    Bsim,
    /// `cap`.
    Cap,
    /// `cccs` (F).
    Cccs,
    /// `ccvs` (H).
    Ccvs,
    /// `cpl`.
    Cpl,
    /// `csw` (W).
    Csw,
    /// `dio`.
    Dio,
    /// `hfeta` .. `hsmhv2`.
    Hfet,
    /// `ind`.
    Ind,
    /// `mut` (K).
    Mut,
    /// `isrc`.
    Isrc,
    /// `jfet` (level 1).
    Jfet,
    /// `jfet2` (level 2).
    Jfet2,
    /// `ltra` (O).
    Ltra,
    /// `mes` (Z level 1).
    Mes,
    /// `mesa` (Z levels 2-4).
    Mesa,
    /// `mos1`.
    Mos1,
    /// `mos2`.
    Mos2,
    /// `mos3`.
    Mos3,
    /// `mos6`.
    Mos6,
    /// `mos9`.
    Mos9,
    /// `res`.
    Res,
    /// `nport`.
    Nport,
    /// `soi3`.
    Soi3,
    /// `sw` (S).
    Sw,
    /// `tra` (T).
    Tra,
    /// `txl` (Y).
    Txl,
    /// `vbic`.
    Vbic,
    /// `vccs` (G).
    Vccs,
    /// `vcvs` (E).
    Vcvs,
    /// `vsrc`.
    Vsrc,
    /// `vdmos`.
    Vdmos,
    /// XSPICE code models, registered after the built-in table.
    CodeModel,
}

impl CDeviceType {
    /// The type of a built-in device letter with a single C type, as `INP2*`
    /// files map them; `None` for letters whose type depends on the model
    /// (M, J, Z, Q's VBIC) or that the port has no type for.
    #[must_use]
    pub const fn of_designator(designator: char) -> Option<Self> {
        Some(match designator.to_ascii_lowercase() {
            'b' => Self::Asrc,
            'q' => Self::Bjt,
            'c' => Self::Cap,
            'f' => Self::Cccs,
            'h' => Self::Ccvs,
            'p' => Self::Cpl,
            'w' => Self::Csw,
            'd' => Self::Dio,
            'l' => Self::Ind,
            'k' => Self::Mut,
            'i' => Self::Isrc,
            'o' => Self::Ltra,
            'r' => Self::Res,
            'n' => Self::Nport,
            's' => Self::Sw,
            't' => Self::Tra,
            'y' => Self::Txl,
            'g' => Self::Vccs,
            'e' => Self::Vcvs,
            'v' => Self::Vsrc,
            'u' => Self::Urc,
            'a' => Self::CodeModel,
            _ => return None,
        })
    }
}

/// Which C model list a device instance belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SetupKey {
    pub(crate) device_type: CDeviceType,
    /// The model name (lowercase, after subcircuit scoping), or `None` for
    /// the type's default model.
    pub(crate) model: Option<String>,
    /// For an element `URCsetup` creates (`urcsetup.c`), the ordinal of its
    /// URC line: its model is created during setup, after every parsed one.
    pub(crate) generated_by: Option<usize>,
}

/// The `CKTsetup`/`CKTload` traversal of devices given in parse (card)
/// order: by C type, then models from the last created to the first (a model
/// is created at its first instance), then instances from the last to the
/// first.
///
/// `URCsetup` runs first (the URC type leads `static_devices[]`) and creates
/// one resistor, capacitor or diode model per URC line with `CKTmodCrt`, so
/// those models are newer than every parsed model, in the order the URC
/// lines are set up; their instances are created (and prepended) in the
/// order [`crate::devices::urc`] lists them.
pub(crate) fn setup_order(keys: &[SetupKey]) -> Vec<usize> {
    let traverse = |created: &[usize]| {
        let mut order: Vec<usize> = (0..keys.len()).collect();
        order.sort_by(|&a, &b| {
            keys[a]
                .device_type
                .cmp(&keys[b].device_type)
                .then(created[b].cmp(&created[a]))
                .then(b.cmp(&a))
        });
        order
    };
    let mut first_reference = std::collections::HashMap::new();
    let mut created: Vec<usize> = keys
        .iter()
        .enumerate()
        .map(|(index, key)| *first_reference.entry(key).or_insert(index))
        .collect();
    if keys.iter().all(|key| key.generated_by.is_none()) {
        return traverse(&created);
    }
    // The URC lines' own setup order decides when their models are made.
    let parsed = traverse(&created);
    for (index, key) in keys.iter().enumerate() {
        if let Some(line) = key.generated_by {
            let rank = parsed.iter().position(|&i| i == line).unwrap_or(line);
            created[index] = keys.len() + rank;
        }
    }
    traverse(&created)
}

/// One device's matrix allocation (`DEVsetup`'s `TSTALLOC` calls).
#[derive(Debug)]
pub struct MatrixSetup<'a> {
    pub(crate) unknowns: &'a MnaUnknowns,
    /// Branch-current rows allocated to this device, in order.
    pub branches: Range<usize>,
    /// Branch rows of the devices named by
    /// [`crate::devices::Device::controlling_sources`], in that order.
    pub controls: &'a [usize],
    /// For a K device, the branch rows of each inductor pair it couples, in
    /// C's instance order.
    pub coupled: &'a [(usize, usize)],
    /// Allocating for pole-zero analysis (`DEVpzSetup`, run by
    /// `CKTpzSetup` on a fresh matrix) rather than `DEVsetup`.
    pub pole_zero: bool,
    pub(crate) elements: &'a mut Vec<(usize, usize)>,
}

impl MatrixSetup<'_> {
    /// The matrix row of a node, `None` for ground.
    #[must_use]
    pub fn node(&self, node: NodeId) -> Option<usize> {
        self.unknowns.node_row(node)
    }

    /// The `index`-th branch row of this device.
    ///
    /// # Errors
    /// [`SpiceError::Circuit`] when the device has no such branch.
    pub fn branch(&self, index: usize) -> SpiceResult<usize> {
        self.branches
            .clone()
            .nth(index)
            .ok_or_else(|| SpiceError::circuit("missing branch-row binding"))
    }

    /// `TSTALLOC(row, col)`: allocates the element unless either index is
    /// ground (SPARSE returns its trash can for row or column 0).
    pub fn element(&mut self, row: Option<usize>, col: Option<usize>) {
        if let (Some(row), Some(col)) = (row, col) {
            self.elements.push((row, col));
        }
    }

    /// [`Self::element`] between two nodes.
    pub fn nodes(&mut self, row: NodeId, col: NodeId) {
        self.element(self.node(row), self.node(col));
    }

    /// `indsetup.c` for an inductor whose branch is `branch`:
    /// `(p,b) (n,b) (b,n) (b,p) (b,b)`.
    pub fn inductor(&mut self, positive: NodeId, negative: NodeId, branch: usize) {
        self.element(self.node(positive), Some(branch));
        self.element(self.node(negative), Some(branch));
        self.element(Some(branch), self.node(negative));
        self.element(Some(branch), self.node(positive));
        self.element(Some(branch), Some(branch));
    }

    /// `vsrcset.c`/`ccvsset.c` voltage-defined branch:
    /// `(p,b) (n,b) (b,n) (b,p)`.
    pub fn voltage_branch(&mut self, positive: NodeId, negative: NodeId, branch: usize) {
        self.element(self.node(positive), Some(branch));
        self.element(self.node(negative), Some(branch));
        self.element(Some(branch), self.node(negative));
        self.element(Some(branch), self.node(positive));
    }

    /// The four elements of a two-terminal conductance in C's usual order:
    /// `(p,p) (n,n) (p,n) (n,p)` (`ressetup.c`, `capsetup.c`).
    pub fn conductance(&mut self, positive: NodeId, negative: NodeId) {
        self.nodes(positive, positive);
        self.nodes(negative, negative);
        self.nodes(positive, negative);
        self.nodes(negative, positive);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(device_type: CDeviceType, model: Option<&str>) -> SetupKey {
        SetupKey {
            device_type,
            model: model.map(str::to_owned),
            generated_by: None,
        }
    }

    #[test]
    fn types_in_table_order_models_and_instances_reversed() {
        let keys = [
            key(CDeviceType::Vsrc, None),        // 0
            key(CDeviceType::Res, None),         // 1
            key(CDeviceType::Res, Some("rm")),   // 2
            key(CDeviceType::Res, None),         // 3
            key(CDeviceType::Cap, None),         // 4
            key(CDeviceType::Res, Some("rm")),   // 5
            key(CDeviceType::Vsrc, None),        // 6
            key(CDeviceType::Res, Some("late")), // 7
        ];
        // cap, then res models newest first: late (7), rm (2, 5), default
        // (1, 3), each instance list reversed; vsrc last.
        assert_eq!(setup_order(&keys), vec![4, 7, 5, 2, 3, 1, 6, 0]);
    }
}
