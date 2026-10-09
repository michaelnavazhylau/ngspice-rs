//! RC step response with an explicitly selected diffsol BDF backend.
use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, runner};
use ngspice_rs::devices::{Circuit, IndependentSource, Waveform};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, Complex, SpiceResult};
use std::path::Path;

fn main() -> SpiceResult<()> {
    let deck = parse_deck_text(
        Path::new("rc.cir"),
        "RC\nv1 in 0 0\nr1 in out 1k\nc1 out 0 1u\n.end\n",
    );
    let netlist = Parser::new().parse_deck(&deck)?;
    let mut circuit = Circuit::from_netlist(&netlist)?;
    let terminals = circuit.devices()[0].terminals();
    let terminals = [terminals[0], terminals[1]];
    // Waveforms currently use the device API; netlist waveform syntax remains
    // a separate parser milestone. DC and AC source syntax is already supported.
    circuit.devices_mut()[0] = Box::new(IndependentSource::new(
        "v1",
        terminals,
        true,
        0.0,
        Complex::ZERO,
        Waveform::Step {
            before: 0.0,
            after: 1.0,
            time: 0.001,
        },
    )?);
    let request = AnalysisRequest::with_arguments(
        AnalysisKind::Transient,
        ["0.1m", "6m", "0", "50u", "backend=diffsol", "method=bdf"],
    );
    let plot = runner(request.kind)?.run(&mut circuit, &request, &AnalysisContext::default())?;
    println!("time(s),v(out)");
    for i in 0..plot.point_count() {
        println!(
            "{},{}",
            plot.value("time", i).unwrap().re,
            plot.value("v(out)", i).unwrap().re
        );
    }
    Ok(())
}
