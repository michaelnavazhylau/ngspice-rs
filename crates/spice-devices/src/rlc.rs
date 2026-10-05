//! Resistor, capacitor and inductor — the first porting targets.
//!
//! These are the device types the port will build first (milestone M2 in
//! `docs/port/ROADMAP.md`), so their shape is fixed even though the stamping is
//! not implemented. Each one follows the same pattern as its C counterpart:
//!
//! | Rust | C parser | C stamping |
//! | --- | --- | --- |
//! | [`Resistor`] | `src/spicelib/parser/inp2r.c` | `src/spicelib/devices/res/resload.c` |
//! | [`Capacitor`] | `src/spicelib/parser/inp2c.c` | `src/spicelib/devices/cap/capload.c` |
//! | [`Inductor`] | `src/spicelib/parser/inp2l.c` | `src/spicelib/devices/ind/indload.c` |
//!
//! The value fields are the *parsed* values. ngspice additionally supports
//! temperature coefficients (`tc1`, `tc2`), behavioural values (`R={expr}`) and
//! instance parameters (`m`, `ac`, `temp`); those are not modelled yet and their
//! absence is the reason the factories in [`crate::registry`] are still stubs.

use spice_core::{NodeId, Real, SpiceError, SpiceResult};
use spice_maths::Vector;

use crate::traits::{Device, StampContext};

/// A resistor, `r1 n1 n2 <value> [tc1=… tc2=…]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Resistor {
    name: String,
    terminals: [NodeId; 2],
    /// Resistance in ohms, as parsed. A behavioural `R={expr}` value is not
    /// modelled yet.
    resistance: Real,
}

impl Resistor {
    /// Builds a resistor.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Circuit`] when the resistance is not finite.
    pub fn new(
        name: impl Into<String>,
        terminals: [NodeId; 2],
        resistance: Real,
    ) -> SpiceResult<Self> {
        let name = name.into();
        if !resistance.is_finite() {
            return Err(SpiceError::circuit(format!(
                "resistor {name}: resistance is not finite"
            )));
        }
        Ok(Self {
            name,
            terminals,
            resistance,
        })
    }

    /// The resistance in ohms.
    #[must_use]
    pub const fn resistance(&self) -> Real {
        self.resistance
    }

    /// The conductance ngspice stamps, `1/R` (`resload.c` computes it as
    /// `1/model->RESconduct`).
    #[must_use]
    pub fn conductance(&self) -> Real {
        1.0 / self.resistance
    }
}

impl Device for Resistor {
    fn name(&self) -> &str {
        &self.name
    }

    fn designator(&self) -> char {
        'r'
    }

    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }

    fn stamp(&mut self, _context: &mut StampContext<'_>) -> SpiceResult<()> {
        Err(SpiceError::not_yet_ported(
            "resistor stamping",
            "src/spicelib/devices/res/resload.c",
        ))
    }
}

/// A capacitor, `c1 n1 n2 <value> [ic=…]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Capacitor {
    name: String,
    terminals: [NodeId; 2],
    /// Capacitance in farads.
    capacitance: Real,
    /// Initial voltage, from `ic=`; `None` when not given.
    initial_voltage: Option<Real>,
}

impl Capacitor {
    /// Builds a capacitor.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Circuit`] when the capacitance is not finite.
    pub fn new(
        name: impl Into<String>,
        terminals: [NodeId; 2],
        capacitance: Real,
        initial_voltage: Option<Real>,
    ) -> SpiceResult<Self> {
        let name = name.into();
        if !capacitance.is_finite() {
            return Err(SpiceError::circuit(format!(
                "capacitor {name}: capacitance is not finite"
            )));
        }
        Ok(Self {
            name,
            terminals,
            capacitance,
            initial_voltage,
        })
    }

    /// The capacitance in farads.
    #[must_use]
    pub const fn capacitance(&self) -> Real {
        self.capacitance
    }

    /// The `ic=` initial voltage, if any.
    #[must_use]
    pub const fn initial_voltage(&self) -> Option<Real> {
        self.initial_voltage
    }
}

impl Device for Capacitor {
    fn name(&self) -> &str {
        &self.name
    }

    fn designator(&self) -> char {
        'c'
    }

    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }

    /// A capacitor is an open circuit at DC, so it adds no unknown.
    fn branch_currents(&self) -> usize {
        0
    }

    fn stamp(&mut self, _context: &mut StampContext<'_>) -> SpiceResult<()> {
        Err(SpiceError::not_yet_ported(
            "capacitor stamping (companion model)",
            "src/spicelib/devices/cap/capload.c, src/maths/ni/niinteg.c",
        ))
    }

    fn accept(&mut self, _solution: &Vector) -> SpiceResult<()> {
        Err(SpiceError::not_yet_ported(
            "capacitor charge history",
            "src/spicelib/devices/cap/capaccept.c",
        ))
    }
}

/// An inductor, `l1 n1 n2 <value> [ic=…]`.
///
/// An inductor is a short at DC, which makes it a voltage-source-like element:
/// it contributes a branch-current unknown, like `V` sources do.
#[derive(Debug, Clone, PartialEq)]
pub struct Inductor {
    name: String,
    terminals: [NodeId; 2],
    /// Inductance in henries.
    inductance: Real,
    /// Initial current, from `ic=`; `None` when not given.
    initial_current: Option<Real>,
}

impl Inductor {
    /// Builds an inductor.
    ///
    /// # Errors
    ///
    /// [`SpiceError::Circuit`] when the inductance is not finite.
    pub fn new(
        name: impl Into<String>,
        terminals: [NodeId; 2],
        inductance: Real,
        initial_current: Option<Real>,
    ) -> SpiceResult<Self> {
        let name = name.into();
        if !inductance.is_finite() {
            return Err(SpiceError::circuit(format!(
                "inductor {name}: inductance is not finite"
            )));
        }
        Ok(Self {
            name,
            terminals,
            inductance,
            initial_current,
        })
    }

    /// The inductance in henries.
    #[must_use]
    pub const fn inductance(&self) -> Real {
        self.inductance
    }

    /// The `ic=` initial current, if any.
    #[must_use]
    pub const fn initial_current(&self) -> Option<Real> {
        self.initial_current
    }
}

impl Device for Inductor {
    fn name(&self) -> &str {
        &self.name
    }

    fn designator(&self) -> char {
        'l'
    }

    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }

    fn branch_currents(&self) -> usize {
        1
    }

    fn stamp(&mut self, _context: &mut StampContext<'_>) -> SpiceResult<()> {
        Err(SpiceError::not_yet_ported(
            "inductor stamping (companion model)",
            "src/spicelib/devices/ind/indload.c",
        ))
    }

    fn accept(&mut self, _solution: &Vector) -> SpiceResult<()> {
        Err(SpiceError::not_yet_ported(
            "inductor flux history",
            "src/spicelib/devices/ind/indaccept.c",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{Capacitor, Inductor, Resistor};
    use crate::traits::Device;
    use spice_core::NodeId;

    fn nodes() -> [NodeId; 2] {
        [NodeId::new(1), NodeId::GROUND]
    }

    #[test]
    fn a_resistor_knows_its_conductance() {
        let resistor = Resistor::new("r1", nodes(), 1000.0).unwrap();
        assert_eq!(resistor.name(), "r1");
        assert_eq!(resistor.designator(), 'r');
        assert_eq!(resistor.terminals(), &nodes());
        assert_eq!(resistor.resistance(), 1000.0);
        assert_eq!(resistor.conductance(), 0.001);
        assert_eq!(resistor.branch_currents(), 0);
        assert!(!resistor.is_nonlinear());
    }

    #[test]
    fn a_capacitor_records_its_initial_condition() {
        let capacitor = Capacitor::new("c1", nodes(), 1e-6, Some(1.5)).unwrap();
        assert_eq!(capacitor.capacitance(), 1e-6);
        assert_eq!(capacitor.initial_voltage(), Some(1.5));
        assert_eq!(capacitor.designator(), 'c');
        assert_eq!(
            Capacitor::new("c1", nodes(), 1e-6, None)
                .unwrap()
                .initial_voltage(),
            None
        );
    }

    #[test]
    fn an_inductor_adds_a_branch_current() {
        let inductor = Inductor::new("l1", nodes(), 1e-3, Some(0.2)).unwrap();
        assert_eq!(inductor.inductance(), 1e-3);
        assert_eq!(inductor.initial_current(), Some(0.2));
        assert_eq!(inductor.designator(), 'l');
        assert_eq!(inductor.branch_currents(), 1);
    }

    #[test]
    fn non_finite_values_are_rejected_at_construction() {
        assert!(Resistor::new("r1", nodes(), f64::INFINITY).is_err());
        assert!(Capacitor::new("c1", nodes(), f64::NAN, None).is_err());
        assert!(Inductor::new("l1", nodes(), f64::INFINITY, None).is_err());
    }

    #[test]
    fn stamping_reports_that_it_is_missing() {
        let mut resistor = Resistor::new("r1", nodes(), 1000.0).unwrap();
        // The specific C file matters: it is what the next contributor needs.
        assert!(stamp_error(&mut resistor).contains("res/resload.c"));
        let mut capacitor = Capacitor::new("c1", nodes(), 1e-6, None).unwrap();
        assert!(stamp_error(&mut capacitor).contains("cap/capload.c"));
        let mut inductor = Inductor::new("l1", nodes(), 1e-3, None).unwrap();
        assert!(stamp_error(&mut inductor).contains("ind/indload.c"));
    }

    fn stamp_error(device: &mut dyn Device) -> String {
        use spice_core::NodeTable;
        use spice_maths::{SparseMatrix, Vector};
        let nodes = NodeTable::new();
        let unknowns = crate::traits::MnaUnknowns::new();
        let mut matrix = SparseMatrix::new(1, 1);
        let mut rhs = Vector::zeros(1);
        let solution = Vector::zeros(1);
        let mut context = crate::traits::StampContext {
            matrix: &mut matrix,
            rhs: &mut rhs,
            unknowns: &unknowns,
            nodes: &nodes,
            solution: &solution,
            temperature: 27.0,
            mode: crate::traits::AnalysisMode::OperatingPoint,
        };
        device.stamp(&mut context).unwrap_err().to_string()
    }
}
