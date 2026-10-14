//! `.tran` backend dispatch.
//!
//! * no `backend=` (or `backend=companion`): the SPICE-compatible adaptive
//!   trapezoidal / Gear-2 companion driver ([`crate::analysis::companion`]),
//!   ngspice's own transient method (`dctran.c`);
//! * `backend=diffsol`: rejected. The native-solver build has no diffsol BDF
//!   backend; ngspice has no BDF integrator either.
use crate::analysis::linear::unsupported;
use crate::analysis::{AnalysisRequest, Plot, PointSink};
use crate::devices::Circuit;
use crate::primitives::SpiceResult;

pub(crate) fn run(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &crate::analysis::AnalysisContext,
) -> SpiceResult<Plot> {
    check_backend(request)?;
    crate::analysis::companion::companion_transient(circuit, request, context)
        .map(|(plot, _stats)| plot)
}

/// [`run`], streaming the accepted points into `sink`; the returned plot has
/// no points.
pub(crate) fn run_into(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &crate::analysis::AnalysisContext,
    sink: &mut dyn PointSink,
) -> SpiceResult<Plot> {
    check_backend(request)?;
    crate::analysis::companion::companion_transient_into(circuit, request, context, sink)
        .map(|(plot, _stats)| plot)
}

/// Accepts the companion driver (no `backend=`, or `backend=companion`) and
/// rejects everything else.
fn check_backend(request: &AnalysisRequest) -> SpiceResult<()> {
    match request.named("backend") {
        None => Ok(()),
        Some(name) if name.eq_ignore_ascii_case("companion") => Ok(()),
        Some(name) if name.eq_ignore_ascii_case("diffsol") => Err(unsupported(
            "backend=diffsol: the adaptive BDF backend was removed with diffsol; ngspice has \
             no BDF integrator. Omit backend= to run the trapezoidal/Gear companion driver \
             (dctran.c)",
        )),
        Some(name) => Err(unsupported(format!(
            "unknown transient backend '{name}'; expected companion (default)"
        ))),
    }
}
