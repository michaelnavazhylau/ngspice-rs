//! Linear analysis orchestration. Nonlinear SPICE convergence policies stay separate.
use crate::results::{PlotFlags, Variable};
use crate::{AnalysisRequest, Plot};
use spice_core::{Complex, Real, SpiceError, SpiceResult, parse_spice_number};
use spice_devices::Circuit;

pub(crate) fn unsupported(feature: impl Into<String>) -> SpiceError {
    SpiceError::Unsupported {
        feature: feature.into(),
        location: None,
    }
}
pub(crate) fn number(text: Option<&str>, name: &str) -> SpiceResult<Real> {
    text.and_then(parse_spice_number)
        .filter(|v| v.is_finite())
        .ok_or_else(|| unsupported(format!("expected finite {name}")))
}

pub(crate) fn plot(
    circuit: &Circuit,
    name: &str,
    title: &str,
    scale: Option<(&str, &str)>,
    complex: bool,
) -> SpiceResult<Plot> {
    let mut plot = Plot::new(
        name,
        title,
        if complex {
            PlotFlags::Complex
        } else {
            PlotFlags::Real
        },
    );
    let var = |name: String, unit: &str| {
        if complex {
            Variable::complex(name, unit)
        } else {
            Variable::new(name, unit)
        }
    };
    if let Some((name, unit)) = scale {
        plot.push_variable(var(name.into(), unit));
    }
    for node in circuit.nodes().nodes() {
        if circuit.unknowns().node_row(node.id).is_some() {
            plot.push_variable(var(format!("v({})", node.name), "voltage"));
        }
    }
    for (index, device) in circuit.devices().iter().enumerate() {
        let range = circuit.branch_rows(index).unwrap();
        if range.len() > 1 {
            return Err(unsupported(
                "multiple branch currents per device in linear plots",
            ));
        }
        if !range.is_empty() {
            plot.push_variable(var(format!("i({})", device.name()), "current"));
        }
    }
    Ok(plot)
}

pub(crate) fn op(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &crate::AnalysisContext,
) -> SpiceResult<Plot> {
    if request
        .arguments
        .iter()
        .any(|argument| !argument.contains('='))
    {
        return Err(unsupported(".op arguments"));
    }
    // .ic is a transient-only constraint in C (cktload.c, MODETRANOP) and a
    // .nodeset cannot change a linear operating point; both are validated.
    circuit.finalize()?;
    let hints = crate::initial::resolve(circuit, request)?;
    let mut seed = spice_maths::Vector::zeros(circuit.unknown_count());
    for hint in hints.nodesets {
        seed.as_mut_slice()[hint.row] = hint.value;
    }
    let x = crate::bias::solve_dc_with(
        circuit,
        &context.model_context(),
        &crate::bias::DcSettings::from_request(request)?,
        &[],
        Some(&seed),
        None,
    )?
    .solution
    .values;
    let mut plot = plot(circuit, "op1", "Operating Point", None, false)?;
    circuit.accept_solution(&x, None)?;
    plot.push_point(x.as_slice().iter().map(|v| Complex::real(*v)).collect())?;
    Ok(plot)
}

pub(crate) fn dc(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &crate::AnalysisContext,
) -> SpiceResult<Plot> {
    crate::sweep::run(circuit, request, context)
}
