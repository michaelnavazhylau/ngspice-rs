//! `.sp` — S-parameter analysis over RF port sources.
//!
//! Ported from `src/spicelib/analysis/span.c` (`SPan`), `cktspdum.c`
//! (`CKTspCalcPowerWave`, `CKTspCalcSMatrix`, `CKTspDump`) and the port hooks
//! of `src/spicelib/devices/vsrc/vsrcacld.c` (`VSRCspinit`, `VSRCspupdate`),
//! an `RFSPICE` build option of ngspice:
//!
//! 1. the operating point and small-signal system are prepared exactly as for
//!    `.ac` ([`crate::analysis::ac::SmallSignal`]), on the same frequency grid;
//! 2. in `MODESP` every independent AC excitation is off (`VSRCacLoad` loads
//!    zero for every voltage source); at each frequency each port `j` in turn
//!    drives a unit voltage into its ideal source branch (`VSRCspupdate`)
//!    while every port keeps its series `z0` (see
//!    [`crate::devices::sources`]);
//! 3. for every port `i`, the terminal voltage `V` and the current `I` into
//!    the positive terminal give the power waves `a = ki (V + z0 I)` and
//!    `b = ki (V - z0 I)`, `ki = 1/(2 sqrt(z0))`, filling column `j` of `A`
//!    and `B`;
//! 4. `S = B A^-1`, `Z = Gn^-1 (E - S)^-1 (S Z0 + Z0) Gn` and
//!    `Y = Gn^-1 (S Z0 + Z0)^-1 (E - S) Gn` with `Z0 = diag(z0)` and
//!    `Gn = diag(2 ki)`.
//!
//! The plot (`SP Analysis`, complex) holds the frequency, the node voltages
//! and branch currents of the **last** port's excitation (C dumps
//! `CKTrhsOld` after the final `NIspSolve`), then `S_i_j`, `Y_i_j`, `Z_i_j`
//! (row `i` = response port, column `j` = driven port) and `v(Rbase)`, the
//! reference impedance of port 1. See `docs/port/SPARAM.md` for the
//! deliberate divergences: RF noise follows C's covariance path; AC current sources with a
//! nonzero imaginary phasor (which C's `span.c` copies into every port
//! excitation) are refused, and a Y or Z matrix that does not exist (a
//! singular conversion) is written as zeros, C's `cinverse` contract for an
//! exactly singular matrix.

use crate::analysis::ac::{SmallSignal, frequency_grid};
use crate::analysis::linear::{plot, unsupported};
use crate::analysis::results::Variable;
use crate::analysis::{AnalysisRequest, Plot};
use crate::devices::Circuit;
use crate::devices::linear::SourceKind;
use crate::maths::complex::ComplexMatrix;
use crate::maths::dense_complex::DenseComplex;
use crate::primitives::{Complex, Real, SpiceError, SpiceResult, parse_spice_number};

/// One RF port bound to matrix rows.
#[derive(Debug, Clone)]
struct BoundPort {
    number: usize,
    z0: Real,
    ki: Real,
    positive: Option<usize>,
    negative: Option<usize>,
    branch: usize,
}

impl BoundPort {
    fn voltage(&self, x: &[Complex]) -> Complex {
        let at = |row: Option<usize>| row.map_or(Complex::ZERO, |row| x[row]);
        at(self.positive) - at(self.negative)
    }
}

pub(crate) fn run(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &crate::analysis::AnalysisContext,
) -> SpiceResult<Plot> {
    let settings = crate::analysis::bias::DcSettings::from_request(request)?;
    let positional: Vec<String> = request
        .arguments
        .iter()
        .filter(|a| !a.contains('='))
        .cloned()
        .collect();
    if !matches!(positional.len(), 4 | 5) {
        return Err(unsupported(".sp lin|dec|oct points start stop [donoise]"));
    }
    let mut donoise = false;
    if let Some(flag) = positional.get(4) {
        // dot_sp() reads donoise as IF_INTEGER, (int) floor(0.5 + value), and
        // SPsetParm enables noise only for exactly 1.
        let value = parse_spice_number(flag)
            .filter(|v| v.is_finite())
            .ok_or_else(|| unsupported(format!(".sp donoise must be an integer, found {flag}")))?;
        donoise = (value + 0.5).floor() == 1.;
    }

    let grid = frequency_grid(&positional)?;
    circuit.finalize()?;
    let ports = bind_ports(circuit)?;
    let mut small_signal = SmallSignal::prepare(circuit, request, context, settings)?;
    reject_imaginary_current_excitation(small_signal.system())?;
    let n = circuit.unknown_count();
    let mut plot = plot(
        circuit,
        "sp1",
        "SP Analysis",
        Some(("frequency", "frequency")),
        true,
    )?;
    for (prefix, unit) in [("S", "s-param"), ("Y", "admittance"), ("Z", "impedance")] {
        for i in 1..=ports.len() {
            for j in 1..=ports.len() {
                plot.push_variable(Variable::complex(format!("{prefix}_{i}_{j}"), unit));
            }
        }
    }
    plot.push_variable(Variable::complex("v(Rbase)", "voltage"));
    if donoise {
        for i in 1..=ports.len() {
            for j in 1..=ports.len() {
                plot.push_variable(Variable::complex(format!("i(Cy_{i}_{j})"), "current"));
            }
        }
        if ports.len() == 2 {
            for (name, unit) in [
                ("NF", "decibel"),
                ("SOpt", "notype"),
                ("NFmin", "decibel"),
                ("Rn", "impedance"),
            ] {
                plot.push_variable(Variable::complex(name, unit));
            }
        }
    }
    let reference = ports[0].z0;
    for f in grid {
        let (last, matrices, factor) =
            small_signal.at_frequency(circuit, context, f, |system| {
                let matrix = ComplexMatrix::from_operators(
                    &system.a,
                    &system.e,
                    2. * std::f64::consts::PI * f,
                )?
                .factorize()?;
                let mut incident = DenseComplex::zeros(ports.len());
                let mut scattered = DenseComplex::zeros(ports.len());
                let mut last = Vec::new();
                for (column, driven) in ports.iter().enumerate() {
                    // VSRCspupdate: unit excitation in the active port's branch
                    // row, every other source off.
                    let mut rhs = vec![Complex::ZERO; n];
                    rhs[driven.branch] = Complex::real(1.);
                    let x = matrix.solve(&rhs)?;
                    for (row, port) in ports.iter().enumerate() {
                        let v = port.voltage(&x);
                        // The branch current flows from #res through the ideal
                        // source; minus it is the current into the positive
                        // terminal (CKTspCalcPowerWave).
                        let i = -x[port.branch];
                        let zi = Complex::real(port.z0);
                        let ki = Complex::real(port.ki);
                        incident.set(row, column, ki * (v + zi * i))?;
                        scattered.set(row, column, ki * (v - zi * i))?;
                    }
                    last = x;
                }
                Ok((
                    last,
                    port_matrices(&ports, &incident, &scattered, f)?,
                    matrix,
                ))
            })?;
        let mut point = Vec::with_capacity(plot.variables.len());
        point.push(Complex::real(f));
        point.extend(last);
        for matrix in &matrices {
            point.extend_from_slice(matrix.data());
        }
        point.push(Complex::real(reference));
        if donoise {
            let generators = crate::devices::noise::circuit_noise(
                circuit,
                &context.model_context().with_frequency(f),
                small_signal.bias(),
                Some(small_signal.state()),
            )?;
            point.extend(noise_parameters(
                circuit,
                &ports,
                &factor,
                &matrices[1],
                &generators,
                context,
            )?);
        }
        plot.push_point(point)?;
    }
    Ok(plot)
}

/// The circuit's RF ports in port-number order, bound to their rows.
/// `Circuit::finalize` has already checked the numbering is `1..=N`.
fn bind_ports(circuit: &Circuit) -> SpiceResult<Vec<BoundPort>> {
    let mut ports = Vec::new();
    for (index, device) in circuit.devices().iter().enumerate() {
        let Some(port) = device.rf_port() else {
            continue;
        };
        let branch = circuit
            .branch_rows(index)
            .and_then(|rows| (rows.len() == 1).then_some(rows.start))
            .ok_or_else(|| {
                SpiceError::circuit(format!("{}: RF port without a branch row", device.name()))
            })?;
        let row = |terminal: usize| {
            device
                .terminals()
                .get(terminal)
                .and_then(|node| circuit.unknowns().node_row(*node))
        };
        ports.push(BoundPort {
            number: port.number,
            z0: port.z0,
            ki: port.ki(),
            positive: row(0),
            negative: row(1),
            branch,
        });
    }
    if ports.is_empty() {
        // span.c: "Error: No RF Port is present, cannot run sp analysis".
        return Err(SpiceError::circuit(
            "no RF port is present (a V source with portnum), cannot run sp analysis",
        ));
    }
    ports.sort_by_key(|port| port.number);
    if ports
        .iter()
        .enumerate()
        .any(|(index, port)| port.number != index + 1)
    {
        return Err(SpiceError::circuit("RF ports must be numbered 1..=N"));
    }
    Ok(ports)
}

/// `span.c` saves the AC load's RHS once and restores it before each port
/// excitation, but copies the *imaginary* RHS over the real one and starts
/// from a zero imaginary part. In `MODESP` voltage sources load nothing, so a
/// current source's AC phasor reaches the port solves only through its
/// imaginary part, as a real excitation: C's S-parameters then depend on an
/// unrelated source. The port refuses that case. A purely real current-source
/// phasor is discarded by the same copy, as here.
fn reject_imaginary_current_excitation(
    system: &crate::devices::linear::LinearSystem,
) -> SpiceResult<()> {
    if let Some(source) = system
        .sources
        .iter()
        .find(|s| s.kind == SourceKind::Current && s.ac.im != 0.)
    {
        return Err(unsupported(format!(
            "{}: .sp with an AC current source of nonzero imaginary phasor (span.c copies it \
             into every port excitation); remove its ac phase or the source",
            source.name
        )));
    }
    Ok(())
}

/// `CKTspCalcSMatrix`: S from the power waves, then Z and Y. A conversion
/// matrix that is singular (Z of a series element, Y of a shunt element)
/// yields a zero block, C's `cinverse` result for an exactly singular matrix.
fn port_matrices(
    ports: &[BoundPort],
    incident: &DenseComplex,
    scattered: &DenseComplex,
    frequency: Real,
) -> SpiceResult<[DenseComplex; 3]> {
    let n = ports.len();
    let a_inverse = incident.inverse()?.ok_or_else(|| SpiceError::Numerical {
        context: ".sp".to_owned(),
        message: format!("incident power-wave matrix is singular at {frequency} Hz"),
    })?;
    let s = scattered.mul(&a_inverse)?;
    let zref = DenseComplex::diagonal(
        &ports
            .iter()
            .map(|port| Complex::real(port.z0))
            .collect::<Vec<_>>(),
    );
    let gn = DenseComplex::diagonal(
        &ports
            .iter()
            .map(|port| Complex::real(2. * port.ki))
            .collect::<Vec<_>>(),
    );
    let gn_inverse = DenseComplex::diagonal(
        &ports
            .iter()
            .map(|port| Complex::real(1. / (2. * port.ki)))
            .collect::<Vec<_>>(),
    );
    let e_minus_s = DenseComplex::identity(n).sub(&s)?;
    let s_z0_plus_z0 = s.mul(&zref)?.add(&zref)?;
    let z = match e_minus_s.inverse()? {
        Some(inverse) => gn_inverse.mul(&inverse.mul(&s_z0_plus_z0.mul(&gn)?)?)?,
        None => DenseComplex::zeros(n),
    };
    let y = match s_z0_plus_z0.inverse()? {
        Some(inverse) => gn_inverse.mul(&inverse.mul(&e_minus_s.mul(&gn)?)?)?,
        None => DenseComplex::zeros(n),
    };
    Ok([s, y, z])
}

/// C `span.c::NInspIter` / `CKTspnoise`, `nevalsrc.c`: independent generator
/// covariances, transformed from terminated port voltages to short-circuit
/// currents. C's N_GAIN branch does not add flicker generators to Cy.
fn noise_parameters(
    circuit: &Circuit,
    ports: &[BoundPort],
    factor: &crate::maths::complex::ComplexLu,
    y: &DenseComplex,
    generators: &[crate::devices::noise::InstanceNoise],
    context: &crate::analysis::AnalysisContext,
) -> SpiceResult<Vec<Complex>> {
    use crate::devices::noise::{BOLTZMANN, NoiseKind};
    let count = ports.len();
    let n = circuit.unknown_count();
    let mut adjoints = Vec::with_capacity(count);
    for port in ports {
        let mut rhs = vec![Complex::ZERO; n];
        if let Some(row) = port.positive {
            rhs[row] = Complex::real(1.);
        }
        if let Some(row) = port.negative {
            rhs[row] = Complex::real(-1.);
        }
        adjoints.push(factor.solve_transposed(&rhs)?);
    }
    let mut cy = DenseComplex::zeros(count);
    for instance in generators {
        for source in &instance.sources {
            if matches!(source.kind, NoiseKind::Flicker { .. }) {
                continue;
            }
            let density = source.kind.output_density(1., 1.);
            let mut voltage = Vec::with_capacity(count);
            for adjoint in &adjoints {
                let at = |node| {
                    circuit
                        .unknowns()
                        .node_row(node)
                        .map_or(Complex::ZERO, |r| adjoint[r])
                };
                voltage.push(
                    (at(source.nodes[0]) - at(source.nodes[1])) * Complex::real(density.sqrt()),
                );
            }
            let mut current = vec![Complex::ZERO; count];
            for i in 0..count {
                current[i] = voltage[i] * Complex::real(1. / ports[i].z0);
                for (j, v) in voltage.iter().enumerate() {
                    current[i] = current[i] + y.get(i, j).unwrap_or(Complex::ZERO) * *v;
                }
            }
            for i in 0..count {
                for j in 0..count {
                    cy.set(
                        i,
                        j,
                        cy.get(i, j).unwrap_or(Complex::ZERO) + current[i] * current[j].conj(),
                    )?;
                }
            }
        }
    }
    let mut result = cy.data().to_vec();
    if count == 2 {
        let norm = Complex::real(4. * BOLTZMANN * (context.temperature + 273.15));
        let get = |i, j| cy.get(i, j).unwrap_or(Complex::ZERO) / norm;
        let y11 = y.get(0, 0).unwrap_or(Complex::ZERO);
        let y21 = y.get(1, 0).unwrap_or(Complex::ZERO);
        let mut rn = get(1, 1).re / y21.magnitude().powi(2);
        if rn.abs() < 1e-30 {
            rn = 1e-30;
        }
        let mut c22 = get(1, 1);
        if c22.re.abs() < 1e-30 && c22.im.abs() < 1e-30 {
            c22.re = 1e-30;
        }
        let ycor = y11 - (get(0, 1) / c22) * y21;
        let gu = get(0, 0).re - rn * (y11 - ycor).magnitude().powi(2);
        let ys = Complex::new((ycor.re * ycor.re + gu / rn).max(0.).sqrt(), -ycor.im);
        let y0 = Complex::real(1. / ports[0].z0);
        let sopt = (y0 - ys) / (y0 + ys);
        let fmin = 1. + 2. * rn * (ycor.re + ys.re);
        let nf = fmin + rn / y0.re * (y0 - ys).magnitude().powi(2);
        result.extend([
            Complex::real(10. * nf.log10()),
            sopt,
            Complex::real(10. * fmin.log10()),
            Complex::real(rn),
        ]);
    }
    if result.iter().any(|v| !v.is_finite()) {
        return Err(SpiceError::Numerical { context: "S-parameter noise".into(), message: "nonfinite covariance or undefined two-port noise parameters (check Y21 and port coupling)".into() });
    }
    Ok(result)
}
