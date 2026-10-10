//! The device registry: designator letter → factory.
//!
//! ngspice's equivalent is the `if_device` table in `src/frontend/device.c` plus
//! the `INP2<letter>()` functions in `src/spicelib/parser/`. The port keeps the
//! same key — the first letter of the instance name — but makes the entry
//! explicit, so that the port's coverage can be reported programmatically
//! instead of inferred from the presence of C files.
//!
//! Each entry's [`DeviceSupport`] is derived from the factory module's own
//! designator lists (`factory::CARD_FACTORY`, `factory::ELABORATED`), so the table cannot claim more or less than
//! the factories build:
//!
//! - **ported**: R/C/L/V/I, E/F/G/H controlled, B behavioural, K (mutual
//!   inductance) and T (lossless transmission line), built from the card
//!   alone by [`Registry::instantiate`];
//! - **bounded**: D/Q/M/J (diode, Gummel-Poon BJT, MOS1/MOS3, JFET level 1), S/W
//!   switches, X subcircuit instances and U uniform RC lines (expanded into
//!   generated R/C/D elements), which need the deck (a `.model` card or a
//!   `.subckt` definition) and are built by [`crate::devices::Circuit::from_netlist`]
//!   for a documented subset of C's models;
//! - **pending**: everything else, an explicit `NotYetPorted` with the C
//!   reference.
//!
//! Unsupported scalar-device parameters are rejected, never silently ignored.

use std::collections::BTreeMap;

use crate::netlist::RawCard;
use crate::primitives::{NodeTable, SpiceError, SpiceResult};

use crate::devices::traits::Device;

/// Builds a device instance from a card.
///
/// The factory may intern new nodes (device-internal nodes) in the node table.
pub type DeviceFactory = fn(&RawCard, &mut NodeTable) -> SpiceResult<Box<dyn Device>>;

/// Everything the port knows about one device designator.
#[derive(Debug, Clone, Copy)]
pub struct DeviceEntry {
    /// The designator letter, lowercased.
    pub designator: char,
    /// A human-readable name for the device family.
    pub description: &'static str,
    /// The C code that must be ported to implement it.
    pub c_reference: &'static str,
    /// What the port builds for this designator.
    pub support: DeviceSupport,
    /// The factory.
    pub factory: DeviceFactory,
}

/// How much of a designator the port builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceSupport {
    /// The entry's factory builds the device from its card.
    Ported,
    /// Built only while elaborating a deck (it needs a `.model` card or a
    /// `.subckt` definition), for the subset of C's models named by `scope`.
    Bounded {
        /// What is built, e.g. the model levels.
        scope: &'static str,
    },
    /// Not built: instances fail with `NotYetPorted` and the C reference.
    Pending,
}

impl DeviceSupport {
    /// `ported`, `bounded` or `pending`.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ported => "ported",
            Self::Bounded { .. } => "bounded",
            Self::Pending => "pending",
        }
    }

    /// Whether the port builds the device at all (ported or bounded).
    #[must_use]
    pub const fn is_implemented(self) -> bool {
        !matches!(self, Self::Pending)
    }

    /// The support the factory module gives `designator`.
    #[must_use]
    pub fn of(designator: char) -> Self {
        let designator = designator.to_ascii_lowercase();
        if crate::devices::factory::CARD_FACTORY.contains(&designator) {
            Self::Ported
        } else if let Some((_, scope)) = crate::devices::factory::ELABORATED
            .iter()
            .find(|(letter, _)| *letter == designator)
        {
            Self::Bounded { scope }
        } else {
            Self::Pending
        }
    }
}

/// The card-only factory of a bounded entry: the device exists, but only a
/// deck can supply its model or subcircuit definition.
fn elaborated_factory(card: &RawCard, _nodes: &mut NodeTable) -> SpiceResult<Box<dyn Device>> {
    let instance = card
        .first_token()
        .map_or("<unnamed>", |token| token.text.as_str());
    Err(SpiceError::Unsupported {
        feature: format!(
            "device instance '{instance}' needs its deck (.model or .subckt); \
             elaborate it with Circuit::from_netlist"
        ),
        location: Some(card.location.clone()),
    })
}

/// The placeholder factory used by every unported device.
///
/// It names the instance in its error but leaves the C reference empty:
/// [`Registry::instantiate`] fills it in from the entry, so the table above
/// stays the single place that records where a device must be ported from.
fn stub_factory(card: &RawCard, _nodes: &mut NodeTable) -> SpiceResult<Box<dyn Device>> {
    let instance = card
        .first_token()
        .map_or("<unnamed>", |token| token.text.as_str());
    Err(SpiceError::not_yet_ported(
        format!("device instance '{instance}'"),
        "",
    ))
}

/// Device designators the port knows about, with their C references.
///
/// Descriptions are deliberately conservative where the C parameter list is the
/// better documentation: `INP2N`/`INP2P` name their devices only through the
/// model UIDs they create (`"nport"` and `"P"`), so the port does not guess.
const BUILTINS: &[(char, &str, &str)] = &[
    (
        'r',
        "resistor",
        "src/spicelib/parser/inp2r.c; src/spicelib/devices/res/res.c",
    ),
    (
        'c',
        "capacitor",
        "src/spicelib/parser/inp2c.c; src/spicelib/devices/cap/cap.c",
    ),
    (
        'l',
        "inductor",
        "src/spicelib/parser/inp2l.c; src/spicelib/devices/ind/ind.c",
    ),
    (
        'v',
        "independent voltage source",
        "src/spicelib/parser/inp2v.c; src/spicelib/devices/vsrc/vsrc.c",
    ),
    (
        'i',
        "independent current source",
        "src/spicelib/parser/inp2i.c; src/spicelib/devices/isrc/isrc.c",
    ),
    (
        'd',
        "diode",
        "src/spicelib/parser/inp2d.c; src/spicelib/devices/dio/dio.c",
    ),
    (
        'q',
        "bipolar transistor (BJT, including VBIC and the NPN variants)",
        "src/spicelib/parser/inp2q.c; src/spicelib/devices/bjt/bjt.c",
    ),
    (
        'm',
        "MOSFET; the model level selects the implementation",
        "src/spicelib/parser/inp2m.c; src/spicelib/devices/mos1/ … src/spicelib/devices/bsim4v7/",
    ),
    (
        'j',
        "JFET",
        "src/spicelib/parser/inp2j.c; src/spicelib/devices/jfet/jfet.c",
    ),
    (
        'z',
        "MESFET, HFET and MESA models",
        "src/spicelib/parser/inp2z.c; src/spicelib/devices/mes/, hfet1/, mesa/",
    ),
    (
        'x',
        "subcircuit instance",
        "src/frontend/inpcom.c (subcircuit expansion); there is no INP2X",
    ),
    (
        'e',
        "voltage-controlled voltage source",
        "src/spicelib/parser/inp2e.c; src/spicelib/devices/vcvs/vcvs.c",
    ),
    (
        'g',
        "voltage-controlled current source",
        "src/spicelib/parser/inp2g.c; src/spicelib/devices/vccs/vccs.c",
    ),
    (
        'f',
        "current-controlled current source",
        "src/spicelib/parser/inp2f.c; src/spicelib/devices/cccs/cccs.c",
    ),
    (
        'h',
        "current-controlled voltage source",
        "src/spicelib/parser/inp2h.c; src/spicelib/devices/ccvs/ccvs.c",
    ),
    (
        'b',
        "behavioural (arbitrary) source",
        "src/spicelib/parser/inp2b.c; src/spicelib/devices/asrc/asrc.c",
    ),
    (
        's',
        "voltage-controlled switch",
        "src/spicelib/parser/inp2s.c; src/spicelib/devices/sw/sw.c",
    ),
    (
        'w',
        "current-controlled switch",
        "src/spicelib/parser/inp2w.c; src/spicelib/devices/csw/csw.c",
    ),
    (
        'k',
        "mutual inductance (transformer coupling)",
        "src/spicelib/parser/inp2k.c; src/spicelib/devices/ind/mutsetup.c, muttemp.c, mutacld.c, indload.c",
    ),
    (
        't',
        "transmission line",
        "src/spicelib/parser/inp2t.c; src/spicelib/devices/tra/tra.c",
    ),
    (
        'o',
        "lossy transmission line",
        "src/spicelib/parser/inp2o.c; src/spicelib/devices/ltra/ltra.c",
    ),
    (
        'y',
        "single lossy transmission line (TXL card)",
        "src/spicelib/parser/inp2y.c; src/spicelib/devices/txl/txl.c",
    ),
    (
        'u',
        "uniform distributed RC line",
        "src/spicelib/parser/inp2u.c; src/spicelib/devices/urc/urc.c",
    ),
    (
        'n',
        "N-device; see INP2N for the parameter list",
        "src/spicelib/parser/inp2n.c",
    ),
    (
        'p',
        "P-device; see INP2P for the parameter list",
        "src/spicelib/parser/inp2p.c",
    ),
    (
        'a',
        "XSPICE code model instance",
        "src/xspice/ (analog model instance)",
    ),
];

/// Maps designator letters to factories.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    entries: BTreeMap<char, DeviceEntry>,
}

impl Registry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Built-in registry: every known designator with the support the
    /// factory module gives it ([`DeviceSupport::of`]).
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        for (designator, description, c_reference) in BUILTINS {
            let support = DeviceSupport::of(*designator);
            registry.register(DeviceEntry {
                designator: *designator,
                description,
                c_reference,
                support,
                factory: match support {
                    DeviceSupport::Ported => crate::devices::factory::from_card,
                    DeviceSupport::Bounded { .. } => elaborated_factory,
                    DeviceSupport::Pending => stub_factory,
                },
            });
        }
        registry
    }

    /// Adds or replaces an entry. Returns the entry it replaced, if any.
    pub fn register(&mut self, entry: DeviceEntry) -> Option<DeviceEntry> {
        self.entries.insert(entry.designator, entry)
    }

    /// The entry for a designator letter, case-insensitively.
    #[must_use]
    pub fn get(&self, designator: char) -> Option<&DeviceEntry> {
        self.entries.get(&designator.to_ascii_lowercase())
    }

    /// Whether a designator is known.
    #[must_use]
    pub fn contains(&self, designator: char) -> bool {
        self.get(designator).is_some()
    }

    /// Number of known designators.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no designators are known.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many known designators the entry's factory builds from the card.
    #[must_use]
    pub fn ported_count(&self) -> usize {
        self.count(|support| support == DeviceSupport::Ported)
    }

    /// How many known designators are built only during deck elaboration.
    #[must_use]
    pub fn bounded_count(&self) -> usize {
        self.count(|support| matches!(support, DeviceSupport::Bounded { .. }))
    }

    fn count(&self, keep: impl Fn(DeviceSupport) -> bool) -> usize {
        self.entries
            .values()
            .filter(|entry| keep(entry.support))
            .count()
    }

    /// The designators, in ascending order.
    pub fn designators(&self) -> impl Iterator<Item = char> + '_ {
        self.entries.keys().copied()
    }

    /// Every entry, in ascending designator order.
    pub fn entries(&self) -> impl Iterator<Item = &DeviceEntry> + '_ {
        self.entries.values()
    }

    /// Builds the device for a card.
    ///
    /// # Errors
    ///
    /// - [`SpiceError::Unsupported`] when the card is not a device instance
    /// - [`SpiceError::UnknownDevice`] when the designator is not registered
    /// - whatever the factory returns; a [`SpiceError::NotYetPorted`] has its C
    ///   reference replaced by the entry's
    pub fn instantiate(
        &self,
        card: &RawCard,
        nodes: &mut NodeTable,
    ) -> SpiceResult<Box<dyn Device>> {
        let Some(designator) = card.designator() else {
            return Err(SpiceError::Unsupported {
                feature: format!("card '{}' is not a device instance", card.raw),
                location: Some(card.location.clone()),
            });
        };
        let Some(entry) = self.get(designator) else {
            let name = card
                .first_token()
                .map_or("<unnamed>", |token| token.text.as_str())
                .to_owned();
            return Err(SpiceError::UnknownDevice {
                name,
                designator,
                location: card.location.clone(),
            });
        };
        match (entry.factory)(card, nodes) {
            Ok(device) => Ok(device),
            Err(SpiceError::NotYetPorted { what, .. }) => {
                Err(SpiceError::not_yet_ported(what, entry.c_reference))
            }
            Err(other) => Err(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DeviceEntry, DeviceSupport, Registry};
    use crate::devices::traits::Device;
    use crate::netlist::{RawCard, source::parse_deck_text};
    use std::path::Path;

    fn card(text: &str) -> RawCard {
        let deck = parse_deck_text(Path::new("test.cir"), &format!("title\n{text}\n"));
        RawCard::parse(&deck.lines[0]).expect("card tokenizes")
    }

    #[test]
    fn builtins_cover_the_device_letters() {
        let registry = Registry::with_builtins();
        for (designator, _, _) in super::BUILTINS {
            assert!(registry.contains(*designator), "missing {designator}");
        }
        assert_eq!(registry.ported_count(), 12);
        assert_eq!(registry.bounded_count(), 8);
        assert_eq!(registry.len(), super::BUILTINS.len());
        for entry in registry.entries() {
            assert!(!entry.description.is_empty(), "{entry:?}");
            assert!(!entry.c_reference.is_empty(), "{entry:?}");
        }
    }

    #[test]
    fn builtin_factories_construct_supported_scalar_devices() {
        let registry = Registry::with_builtins();
        let mut nodes = crate::primitives::NodeTable::new();
        for text in [
            "r1 a 0 1k",
            "c1 a 0 1u",
            "l1 a 0 1m",
            "v1 a 0 dc 1 ac 2 90",
            "i1 0 a 1m",
        ] {
            let device = registry.instantiate(&card(text), &mut nodes).unwrap();
            assert_eq!(device.terminals().len(), 2);
            assert_eq!(
                device.branch_currents(),
                usize::from(matches!(device.designator(), 'v' | 'l'))
            );
        }
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn designators_are_enumerated_in_order() {
        let registry = Registry::with_builtins();
        let designators: Vec<char> = registry.designators().collect();
        let mut sorted = designators.clone();
        sorted.sort_unstable();
        assert_eq!(designators, sorted);
        assert!(designators.contains(&'r'));
        assert!(!designators.contains(&'9'));
    }

    #[test]
    fn designator_lookup_is_case_insensitive() {
        let registry = Registry::with_builtins();
        assert_eq!(registry.get('R').map(|entry| entry.designator), Some('r'));
        assert_eq!(
            registry.get('R').map(|entry| entry.description),
            Some("resistor")
        );
    }

    #[test]
    fn unsupported_instantiation_reports_the_entry_c_reference() {
        let registry = Registry::with_builtins();
        let mut nodes = crate::primitives::NodeTable::new();
        let error = registry
            .instantiate(&card("z1 d g s zm"), &mut nodes)
            .expect_err("not ported");
        assert!(error.is_not_yet_ported());
        let message = error.to_string();
        assert!(message.contains("device instance 'z1'"), "{message}");
        assert!(message.contains("inp2z.c"), "{message}");
        assert!(message.contains("mes/"), "{message}");
        assert!(nodes.is_empty(), "a stub factory must not intern nodes");
    }

    #[test]
    fn unknown_designators_are_reported() {
        let mut nodes = crate::primitives::NodeTable::new();
        let empty = Registry::new();
        assert!(empty.is_empty());
        let error = empty
            .instantiate(&card("l1 a b 1u"), &mut nodes)
            .unwrap_err();
        assert!(matches!(
            error,
            crate::primitives::SpiceError::UnknownDevice {
                designator: 'l',
                ..
            }
        ));
        assert!(Registry::with_builtins().contains('l'));
    }

    #[test]
    fn non_device_cards_are_unsupported() {
        let registry = Registry::with_builtins();
        let mut nodes = crate::primitives::NodeTable::new();
        let error = registry
            .instantiate(&card(".tran 1u 1m"), &mut nodes)
            .unwrap_err();
        assert!(matches!(
            error,
            crate::primitives::SpiceError::Unsupported { .. }
        ));
    }

    #[test]
    fn a_ported_entry_can_be_replaced() {
        #[derive(Debug)]
        struct Nothing;
        impl Device for Nothing {
            fn name(&self) -> &str {
                "nothing"
            }
            fn designator(&self) -> char {
                'r'
            }
            fn terminals(&self) -> &[crate::primitives::NodeId] {
                &[]
            }
            fn stamp(
                &self,
                _context: &mut crate::devices::traits::StampContext<'_>,
            ) -> crate::primitives::SpiceResult<()> {
                Ok(())
            }
        }
        fn factory(
            _card: &RawCard,
            _nodes: &mut crate::primitives::NodeTable,
        ) -> crate::primitives::SpiceResult<Box<dyn Device>> {
            Ok(Box::new(Nothing))
        }

        let mut registry = Registry::with_builtins();
        let replaced = registry.register(DeviceEntry {
            designator: 'r',
            description: "resistor",
            c_reference: "src/spicelib/parser/inp2r.c",
            support: DeviceSupport::Ported,
            factory,
        });
        assert!(replaced.is_some_and(|entry| entry.support == DeviceSupport::Ported));
        assert_eq!(registry.ported_count(), 12);
        let mut nodes = crate::primitives::NodeTable::new();
        assert_eq!(
            registry
                .instantiate(&card("r1 in out 1k"), &mut nodes)
                .expect("ported")
                .name(),
            "nothing"
        );
    }
}
