//! The circuit: nodes, unknowns and device instances.
//!
//! The C equivalent is `CKTcircuit` in `src/include/ngspice/cktdefs.h`, built by
//! `CKTcrtElt`/`CKTbindNode` and friends in `src/spicelib/devices/ckt*.c`. The
//! port keeps the same two-step shape:
//!
//! 1. nodes and devices are *added*, in deck order;
//! 2. [`Circuit::rebuild_unknowns`] numbers the MNA unknowns, which must happen
//!    after every node and device is known, because branch currents get rows
//!    after the nodes.
//!
//! Step 2 is easy to forget, so the analyses call
//! [`Circuit::finalize`], which validates the graph and rebuilds the numbering
//! in one go.

use std::fmt;

use spice_core::{NodeId, NodeTable, SpiceError, SpiceResult};

use crate::traits::{Device, MnaUnknowns};

/// A circuit under construction, or ready to be simulated.
#[derive(Default)]
pub struct Circuit {
    nodes: NodeTable,
    unknowns: MnaUnknowns,
    devices: Vec<Box<dyn Device>>,
}

impl fmt::Debug for Circuit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Circuit")
            .field("nodes", &self.nodes.len())
            .field("unknowns", &self.unknowns.len())
            .field(
                "devices",
                &self
                    .devices
                    .iter()
                    .map(|device| device.name())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl Circuit {
    /// An empty circuit holding only ground.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The node table.
    #[must_use]
    pub fn nodes(&self) -> &NodeTable {
        &self.nodes
    }

    /// The MNA unknown numbering. Only valid after [`Circuit::rebuild_unknowns`].
    #[must_use]
    pub fn unknowns(&self) -> &MnaUnknowns {
        &self.unknowns
    }

    /// The device instances, in insertion order.
    #[must_use]
    pub fn devices(&self) -> &[Box<dyn Device>] {
        &self.devices
    }

    /// The device instances, mutable, so an analysis can refresh them.
    pub fn devices_mut(&mut self) -> &mut [Box<dyn Device>] {
        &mut self.devices
    }

    /// Number of device instances.
    #[must_use]
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Size of the MNA system.
    #[must_use]
    pub fn unknown_count(&self) -> usize {
        self.unknowns.len()
    }

    /// Adds a node, or returns the existing one.
    ///
    /// Matrix rows change as nodes are added, so call
    /// [`Circuit::rebuild_unknowns`] once the circuit is complete.
    pub fn add_node(&mut self, name: &str) -> NodeId {
        self.nodes.intern(name)
    }

    /// Rebuilds the unknown numbering: one row per non-ground node, then one row
    /// per branch current, in device order.
    pub fn rebuild_unknowns(&mut self) {
        self.unknowns.rebuild(&self.nodes);
        for device in &self.devices {
            self.unknowns.add_rows(device.branch_currents());
        }
    }

    /// Validates the graph and renumbers the unknowns.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Circuit`] if a device refers to a node that is not in the
    /// table, or if two devices share an instance name — ngspice rejects the
    /// latter too, in `INP2dot`/`CKTcrtElt`.
    pub fn finalize(&mut self) -> SpiceResult<()> {
        let mut seen: Vec<String> = Vec::with_capacity(self.devices.len());
        for device in &self.devices {
            for terminal in device.terminals() {
                if self.nodes.node(*terminal).is_none() {
                    return Err(SpiceError::circuit(format!(
                        "device {} refers to {terminal}, which is not in the node table",
                        device.name()
                    )));
                }
            }
            let name = device.name().to_lowercase();
            if seen.contains(&name) {
                return Err(SpiceError::circuit(format!(
                    "duplicate instance name '{}'",
                    device.name()
                )));
            }
            seen.push(name);
        }
        self.rebuild_unknowns();
        Ok(())
    }

    /// Adds a device, checking that its terminals are already in the node table
    /// and that its name is not taken.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Circuit`] on a dangling terminal or a duplicate name.
    pub fn add_device(&mut self, device: Box<dyn Device>) -> SpiceResult<()> {
        for terminal in device.terminals() {
            if self.nodes.node(*terminal).is_none() {
                return Err(SpiceError::circuit(format!(
                    "device {} refers to {terminal}, which is not in the node table",
                    device.name()
                )));
            }
        }
        if self.device(device.name()).is_some() {
            return Err(SpiceError::circuit(format!(
                "duplicate instance name '{}'",
                device.name()
            )));
        }
        self.devices.push(device);
        Ok(())
    }

    /// Looks up a device by instance name, case-insensitively.
    #[must_use]
    pub fn device(&self, name: &str) -> Option<&dyn Device> {
        self.devices
            .iter()
            .find(|device| device.name().eq_ignore_ascii_case(name))
            .map(AsRef::as_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::Circuit;
    use crate::traits::Device;
    use spice_core::{NodeId, SpiceError, SpiceResult};
    use spice_maths::Vector;

    /// A test double: a two-terminal device that never stamps.
    #[derive(Debug)]
    struct Stub {
        name: String,
        terminals: [NodeId; 2],
        branch_currents: usize,
    }

    impl Device for Stub {
        fn name(&self) -> &str {
            &self.name
        }

        fn designator(&self) -> char {
            'r'
        }

        fn terminals(&self) -> &[NodeId] {
            &self.terminals
        }

        fn branch_currents(&self) -> usize {
            self.branch_currents
        }

        fn stamp(&mut self, _context: &mut crate::traits::StampContext<'_>) -> SpiceResult<()> {
            Err(SpiceError::not_yet_ported("test stub", "tests"))
        }

        fn accept(&mut self, _solution: &Vector) -> SpiceResult<()> {
            Ok(())
        }
    }

    fn stub(circuit: &mut Circuit, name: &str, a: &str, b: &str, branch_currents: usize) -> Stub {
        let first = circuit.add_node(a);
        let second = circuit.add_node(b);
        Stub {
            name: name.to_owned(),
            terminals: [first, second],
            branch_currents,
        }
    }

    #[test]
    fn unknowns_are_renumbered_after_every_device_is_added() {
        let mut circuit = Circuit::new();
        let device = stub(&mut circuit, "r1", "in", "out", 0);
        circuit.add_device(Box::new(device)).unwrap();
        let source = stub(&mut circuit, "v1", "in", "0", 1);
        circuit.add_device(Box::new(source)).unwrap();

        circuit.finalize().unwrap();
        // Two nodes plus one branch current.
        assert_eq!(circuit.unknown_count(), 3);
        assert_eq!(circuit.device_count(), 2);
        assert_eq!(circuit.device("R1").map(Device::name), Some("r1"));
    }

    #[test]
    fn duplicate_names_are_rejected() {
        let mut circuit = Circuit::new();
        let first = stub(&mut circuit, "r1", "a", "b", 0);
        circuit.add_device(Box::new(first)).unwrap();
        let second = stub(&mut circuit, "R1", "a", "b", 0);
        let error = circuit.add_device(Box::new(second)).unwrap_err();
        assert!(error.to_string().contains("duplicate instance name"));
    }

    #[test]
    fn dangling_terminals_are_rejected() {
        let mut circuit = Circuit::new();
        let known = circuit.add_node("a");
        let dangling = NodeId::new(99);
        let device = Stub {
            name: "r1".to_owned(),
            terminals: [known, dangling],
            branch_currents: 0,
        };
        let error = circuit.add_device(Box::new(device)).unwrap_err();
        assert!(error.to_string().contains("not in the node table"));
    }

    #[test]
    fn finalize_catches_devices_added_behind_its_back() {
        let mut circuit = Circuit::new();
        let device = stub(&mut circuit, "r1", "a", "b", 0);
        circuit.add_device(Box::new(device)).unwrap();
        circuit.finalize().unwrap();
        assert_eq!(circuit.unknown_count(), 2);

        // A node added afterwards shifts nothing until the next rebuild.
        circuit.add_node("late");
        circuit.finalize().unwrap();
        assert_eq!(circuit.unknown_count(), 3);
    }

    #[test]
    fn debug_output_lists_device_names() {
        let mut circuit = Circuit::new();
        let device = stub(&mut circuit, "r1", "a", "b", 0);
        circuit.add_device(Box::new(device)).unwrap();
        let text = format!("{circuit:?}");
        assert!(text.contains("r1"), "{text}");
        assert!(text.contains("nodes: 3"), "{text}");
    }
}
