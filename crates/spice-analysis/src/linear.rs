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
pub(crate) fn accept(circuit: &mut Circuit, x: &spice_maths::Vector) -> SpiceResult<()> {
    for device in circuit.devices_mut() {
        device.accept(x)?;
    }
    Ok(())
}

pub(crate) fn op(circuit: &mut Circuit, request: &AnalysisRequest) -> SpiceResult<Plot> {
    if !request.arguments.is_empty() {
        return Err(unsupported(".op arguments"));
    }
    let system = circuit.linear_system()?;
    let x = system.a.solve(&system.dc_rhs(None)?)?;
    let mut plot = plot(circuit, "op1", "Operating Point", None, false)?;
    accept(circuit, &x)?;
    plot.push_point(x.as_slice().iter().map(|v| Complex::real(*v)).collect())?;
    Ok(plot)
}

pub(crate) fn dc(circuit: &mut Circuit, request: &AnalysisRequest) -> SpiceResult<Plot> {
    if request.arguments.len() != 4 {
        return Err(unsupported(
            ".dc requires one independent source, start, stop, step",
        ));
    }
    let name = request.argument(0).unwrap();
    let start = number(request.argument(1), "sweep start")?;
    let stop = number(request.argument(2), "sweep stop")?;
    let step = number(request.argument(3), "sweep step")?;
    if step == 0. || (stop - start) * step < 0. {
        return Err(unsupported("invalid sweep direction/step"));
    }
    let count = ((stop - start) / step).floor() + 1.;
    if !count.is_finite() || !(1. ..=100_000.).contains(&count) {
        return Err(unsupported("DC sweep point limit exceeded"));
    }
    let system = circuit.linear_system()?;
    let source = system
        .sources
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| unsupported("DC sweep supports independent V/I sources only"))?;
    let unit = if name.to_ascii_lowercase().starts_with('v') {
        "voltage"
    } else {
        "current"
    };
    let mut plot = plot(
        circuit,
        "dc1",
        "DC transfer characteristic",
        Some(("sweep", unit)),
        false,
    )?;
    let lu = system.a.factorize()?;
    for i in 0..count as usize {
        let value = start + (i as f64) * step;
        if !value.is_finite() || (i > 0 && value == start + ((i - 1) as f64) * step) {
            return Err(unsupported("DC sweep makes no progress"));
        }
        let x = lu.solve(&system.dc_rhs(Some((&source.name, value)))?)?;
        accept(circuit, &x)?;
        let mut point = vec![Complex::real(value)];
        point.extend(x.as_slice().iter().map(|v| Complex::real(*v)));
        plot.push_point(point)?;
    }
    Ok(plot)
}
