//! Device observations from the same physical loads used by the solver.
//! C: `frontend/outitf.c`, `spiceif.c::if_getparam`, device `*ask.c` routines.
//! Loads are disposable; no observation accepts state or advances history.
use crate::devices::{Circuit, Device, IterationPhase, LoadRequest, ModelContext};
use crate::maths::{SparseMatrix, Vector};
use crate::primitives::{Complex, Real, SpiceError, SpiceResult};

fn gap(name: &str) -> SpiceError {
    SpiceError::not_yet_ported(
        format!("device observation {name}"),
        "src/frontend/spiceif.c (if_getparam), src/spicelib/devices/*/*ask.c",
    )
}

fn query(name: &str) -> SpiceResult<(&str, &str)> {
    if let Some(device) = name.strip_prefix("i(").and_then(|s| s.strip_suffix(')')) {
        return Ok((device, "i"));
    }
    let (device, parameter) = name
        .strip_prefix('@')
        .and_then(|s| s.split_once('['))
        .ok_or_else(|| gap(name))?;
    let parameter = parameter
        .strip_suffix(']')
        .filter(|s| !s.is_empty() && !s.contains(['[', ']']))
        .ok_or_else(|| gap(name))?;
    if device.is_empty() {
        return Err(gap(name));
    }
    Ok((device, parameter))
}

fn terminal(device: &dyn Device, key: &str) -> Option<usize> {
    match (device.designator(), key) {
        (_, "i" | "current") => Some(0),
        ('d', "id") | ('q', "ic") | ('m', "id") => Some(0),
        ('q', "ib") | ('m', "ig") => Some(1),
        ('q', "ie") | ('m', "is") => Some(2),
        ('m', "ib") => Some(3),
        _ => None,
    }
}

/// Rawfile units of the supported observation quantities.
#[must_use]
pub fn unit(name: &str) -> &'static str {
    let (device, key) = query(name).unwrap_or(("", ""));
    if key == "ic" && device.starts_with('c') {
        return "voltage";
    }
    match key {
        "i" | "current" | "id" | "ic" | "ib" | "ie" | "is" | "ig" => "current",
        "r" | "resistance" => "resistance",
        "conductance" | "g" => "conductance",
        "c" | "cap" | "capacitance" => "capacitance",
        "l" | "inductance" => "inductance",
        "temp" | "dtemp" => "temperature",
        "p" | "power" => "power",
        _ => "parameter",
    }
}

impl Circuit {
    /// Select extra device quantities to record at each solved point.
    /// Existing branch-current columns are retained without duplication.
    ///
    /// # Errors
    /// Unknown devices, malformed queries, or duplicate queries beyond the budget.
    pub fn set_observations(&mut self, names: &[String]) -> SpiceResult<()> {
        let mut out = Vec::new();
        for name in names {
            let name = name.to_ascii_lowercase();
            let (device, _) = query(&name)?;
            let index = self
                .devices()
                .iter()
                .position(|d| d.name().eq_ignore_ascii_case(device))
                .ok_or_else(|| SpiceError::circuit(format!("{name}: unknown device {device}")))?;
            if name.starts_with("i(") && self.devices()[index].branch_currents() == 1 {
                continue;
            }
            if !out.contains(&name) {
                out.push(name);
            }
            if out.len() > 1024 {
                return Err(gap("more than 1024 requested quantities"));
            }
        }
        self.observations = out;
        Ok(())
    }

    /// Extra columns requested through [`Self::set_observations`].
    #[must_use]
    pub fn observations(&self) -> &[String] {
        &self.observations
    }

    /// Observe a converged real point using immutable copies of its device loads.
    /// `sources` carries DC sweep source values; `time` selects transient source forcing.
    /// The pre-acceptance history and coefficients must be those of the solved trial.
    ///
    /// # Errors
    /// Unsupported quantities, ambiguous coincident terminal currents, or nonfinite arithmetic.
    pub fn observe_real(
        &self,
        request: &LoadRequest<'_>,
        sources: &[(&str, Real)],
        time: Option<(Real, crate::devices::Forcing)>,
        previous: Option<&crate::devices::TrialState>,
    ) -> SpiceResult<Vec<Complex>> {
        if self.observations.is_empty() {
            return Ok(Vec::new());
        }
        let replacements = self.replacements(request.model_context)?;
        self.observations
            .iter()
            .map(|name| {
                let (instance, key) = query(name)?;
                let index = self
                    .devices()
                    .iter()
                    .position(|d| d.name().eq_ignore_ascii_case(instance))
                    .ok_or_else(|| {
                        SpiceError::circuit(format!("unknown observation device {instance}"))
                    })?;
                let device: &dyn Device = replacements
                    .iter()
                    .find(|(i, _)| *i == index)
                    .map_or(self.devices()[index].as_ref(), |(_, d)| d.as_ref());
                let power = matches!(key, "p" | "power");
                let value = if let Some(port) = terminal(device, key).or(power.then_some(0)) {
                    if power && device.terminals().len() != 2 {
                        return Err(gap(name));
                    }
                    let node = *device.terminals().get(port).ok_or_else(|| gap(name))?;
                    if device.terminals().iter().filter(|n| **n == node).count() != 1 {
                        return Err(gap(&format!("{name}: coincident terminals")));
                    }
                    let mut matrix = SparseMatrix::new(self.unknown_count(), self.unknown_count());
                    let mut rhs = Vector::zeros(self.unknown_count());
                    let mut trial = request
                        .history
                        .trial_in(IterationPhase::Float, previous)?
                        .with_device_limiting(false);
                    self.load_device(index, device, request, &mut matrix, &mut rhs, &mut trial)?;
                    // Current sources are pure forcing, including multiplicity and swept values.
                    if device.designator() == 'i' {
                        let value = device
                            .observation_source_current(time)?
                            .ok_or_else(|| gap(name))?;
                        let value = sources
                            .iter()
                            .find(|(n, _)| n.eq_ignore_ascii_case(instance))
                            .map_or(value, |(_, v)| *v * device.observation_source_multiplier());
                        for (terminal, sign) in device.terminals().iter().take(2).zip([-1., 1.]) {
                            if let Some(row) = self.unknowns().node_row(*terminal) {
                                rhs.as_mut_slice()[row] = sign * value;
                            }
                        }
                    }
                    let ax = matrix.mul_vector(request.solution)?;
                    let current = if let Some(row) = self.unknowns().node_row(node) {
                        ax.as_slice()[row] - rhs.as_slice()[row]
                    } else {
                        let mut sum = 0.;
                        let mut seen = Vec::new();
                        for other in device.terminals() {
                            if !seen.contains(other) {
                                seen.push(*other);
                                if let Some(row) = self.unknowns().node_row(*other) {
                                    sum -= ax.as_slice()[row] - rhs.as_slice()[row];
                                }
                            }
                        }
                        sum
                    };
                    if power {
                        let voltage = |node| {
                            self.unknowns()
                                .node_row(node)
                                .map_or(0., |r| request.solution.as_slice()[r])
                        };
                        current * (voltage(device.terminals()[0]) - voltage(device.terminals()[1]))
                    } else {
                        current
                    }
                } else if key == "dc" && matches!(device.designator(), 'v' | 'i') {
                    sources
                        .iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case(instance))
                        .map(|(_, value)| *value)
                        .or(device.observation_parameter(key, request.model_context)?)
                        .ok_or_else(|| gap(name))?
                } else {
                    device
                        .observation_parameter(key, request.model_context)?
                        .ok_or_else(|| gap(name))?
                };
                if !value.is_finite() {
                    return Err(SpiceError::Numerical {
                        context: name.clone(),
                        message: "nonfinite device observation".into(),
                    });
                }
                Ok(Complex::real(value))
            })
            .collect()
    }

    /// Observe scalar instance parameters for a spectral point. Device current and
    /// power asks are rejected as by C's passive `*ask.c` routines in AC.
    ///
    /// # Errors
    /// A current/power or unsupported scalar query, or nonfinite parameter.
    pub fn observe_parameters(&self, context: &ModelContext) -> SpiceResult<Vec<Complex>> {
        let replacements = self.replacements(context)?;
        self.observations
            .iter()
            .map(|name| {
                let (instance, key) = query(name)?;
                let index = self
                    .devices()
                    .iter()
                    .position(|d| d.name().eq_ignore_ascii_case(instance))
                    .ok_or_else(|| gap(name))?;
                let device: &dyn Device = replacements
                    .iter()
                    .find(|(i, _)| *i == index)
                    .map_or(self.devices()[index].as_ref(), |(_, d)| d.as_ref());
                if terminal(device, key).is_some() || matches!(key, "p" | "power") {
                    return Err(gap(&format!("{name}: AC current/power ask")));
                }
                let value = device
                    .observation_parameter(key, context)?
                    .ok_or_else(|| gap(name))?;
                if !value.is_finite() {
                    return Err(gap(name));
                }
                Ok(Complex::real(value))
            })
            .collect()
    }
}
