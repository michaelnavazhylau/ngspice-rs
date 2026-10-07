//! Resistor, capacitor and inductor — the first porting targets.
//!
//! Static and state-independent dynamic operators are implemented. The separate
//! trap/Gear companion-model contract remains pending (roadmap M3).
//! Each type follows its C counterpart:
//!
//! | Rust | C parser | C stamping |
//! | --- | --- | --- |
//! | [`Resistor`] | `src/spicelib/parser/inp2r.c` | `src/spicelib/devices/res/resload.c` |
//! | [`Capacitor`] | `src/spicelib/parser/inp2c.c` | `src/spicelib/devices/cap/capload.c` |
//! | [`Inductor`] | `src/spicelib/parser/inp2l.c` | `src/spicelib/devices/ind/indload.c` |
//!
//! Fields hold the supplied scalar values. [`crate::passive`] projects bounded
//! model geometry, TC1/TC2, TEMP/TNOM, scale and multiplicity into effective
//! scalars before delegating here. Literal factory support is unchanged;
//! behavioural values, AC-only resistance and other advanced setters error
//! explicitly rather than silently modifying or omitting physics.

use spice_core::{NodeId, Real, SpiceError, SpiceResult};

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
    /// [`SpiceError::Circuit`] for non-finite or zero resistance/conductance overflow.
    pub fn new(
        name: impl Into<String>,
        terminals: [NodeId; 2],
        resistance: Real,
    ) -> SpiceResult<Self> {
        let name = name.into();
        if !resistance.is_finite() || resistance == 0.0 || !(1.0 / resistance).is_finite() {
            return Err(SpiceError::circuit(format!(
                "resistor {name}: resistance must be finite and nonzero with finite conductance"
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

    fn stamp(&mut self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_ac() {
            return Err(SpiceError::circuit("use complex equation assembly for AC"));
        }
        crate::linear::nodal_stamp(
            context.matrix,
            context.unknowns,
            self.terminals,
            self.conductance(),
        )
    }
    fn assemble_linear(&self, context: &mut crate::linear::LinearContext<'_>) -> SpiceResult<()> {
        context.nodal(self.terminals, self.conductance(), false)
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
    /// [`SpiceError::Circuit`] for nonpositive/non-finite capacitance or non-finite IC.
    pub fn new(
        name: impl Into<String>,
        terminals: [NodeId; 2],
        capacitance: Real,
        initial_voltage: Option<Real>,
    ) -> SpiceResult<Self> {
        let name = name.into();
        if !capacitance.is_finite()
            || capacitance <= 0.0
            || initial_voltage.is_some_and(|v| !v.is_finite())
        {
            return Err(SpiceError::circuit(format!(
                "capacitor {name}: capacitance must be positive and finite; initial voltage must be finite"
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

    fn stamp(&mut self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if context.mode.is_dc() {
            return Ok(());
        }
        Err(SpiceError::not_yet_ported(
            "capacitor companion model",
            "src/spicelib/devices/cap/capload.c, src/maths/ni/niinteg.c",
        ))
    }
    fn assemble_linear(&self, context: &mut crate::linear::LinearContext<'_>) -> SpiceResult<()> {
        context.system.has_initial_conditions |= self.initial_voltage.is_some();
        context.nodal(self.terminals, self.capacitance, true)
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
    /// [`SpiceError::Circuit`] for nonpositive/non-finite inductance or non-finite IC.
    pub fn new(
        name: impl Into<String>,
        terminals: [NodeId; 2],
        inductance: Real,
        initial_current: Option<Real>,
    ) -> SpiceResult<Self> {
        let name = name.into();
        if !inductance.is_finite()
            || inductance <= 0.0
            || initial_current.is_some_and(|v| !v.is_finite())
        {
            return Err(SpiceError::circuit(format!(
                "inductor {name}: inductance must be positive and finite; initial current must be finite"
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

    fn stamp(&mut self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        if !context.mode.is_dc() {
            return Err(SpiceError::not_yet_ported(
                "inductor companion model",
                "src/spicelib/devices/ind/indload.c",
            ));
        }
        let row = context
            .branch
            .ok_or_else(|| SpiceError::circuit("missing inductor branch row"))?;
        crate::linear::branch_stamp(context.matrix, context.unknowns, self.terminals, row)
    }
    fn assemble_linear(&self, context: &mut crate::linear::LinearContext<'_>) -> SpiceResult<()> {
        let branch = context.branch(self.terminals)?;
        // v+ - v- - L di/dt = 0, i positive from + to -.
        context.system.e.add(branch, branch, -self.inductance)?;
        context.system.has_initial_conditions |= self.initial_current.is_some();
        Ok(())
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
    fn companion_stamping_remains_explicitly_unimplemented() {
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
            nominal_temperature: 27.0,
            mode: crate::traits::AnalysisMode::Transient { time: 0., dt: 1e-6 },
            branch: None,
        };
        device.stamp(&mut context).unwrap_err().to_string()
    }
}
