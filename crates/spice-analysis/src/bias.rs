//! Nonlinear DC operating points with bounded continuation (`cktop.c`).
use crate::newton::{self, NewtonOptions, NewtonSolution};
use spice_core::{SpiceError, SpiceResult};
use spice_devices::{AnalysisMode, Circuit, LoadRequest, ModelContext, TrialState};
use spice_maths::{SparseMatrix, Vector};

/// Row kinds shared by DC and companion Newton iteration.
pub(crate) fn branch_rows(circuit: &Circuit) -> Vec<bool> {
    let mut kinds = vec![false; circuit.unknown_count()];
    for i in 0..circuit.device_count() {
        for row in circuit.branch_rows(i).unwrap_or(0..0) {
            kinds[row] = true;
        }
    }
    kinds
}

/// Solve a finalized circuit without committing device history or accept hooks.
///
/// Independent source overrides are typed, checked, and applied to a temporary
/// RHS only. `forcing` replaces source DC values for the transient bias at 0-.
/// Failed direct Newton attempts use a decreasing nodal-gmin schedule, then
/// source stepping. Every successful result is finally solved with **zero
/// artificial nodal gmin and full source values**, so continuation never changes
/// the requested physical solution. The device's own junction gmin is separate.
///
/// # Errors
/// Invalid inputs, device/structural failures, singular final system, or bounded
/// convergence failure. Temporary continuation solutions are never accepted.
pub fn solve_dc(
    circuit: &Circuit,
    context: &ModelContext,
    options: &NewtonOptions,
    overrides: &[(&str, f64)],
    initial: Option<&Vector>,
    forcing: Option<&Vector>,
) -> SpiceResult<NewtonSolution<TrialState>> {
    options.validate()?;
    let n = circuit.unknown_count();
    if initial.is_some_and(|x| x.len() != n || !x.is_finite()) {
        return Err(SpiceError::circuit("invalid DC initial solution"));
    }
    if forcing.is_some() && !overrides.is_empty() {
        return Err(SpiceError::circuit(
            "combine neither source overrides nor transient-bias forcing",
        ));
    }
    let zero = Vector::zeros(n);
    let system = circuit.small_signal_system(context, &zero)?;
    let original = system.dc_rhs(None)?;
    let mut target = forcing.cloned().unwrap_or_else(|| original.clone());
    if target.len() != n || !target.is_finite() {
        return Err(SpiceError::circuit("invalid DC forcing"));
    }
    let mut names = std::collections::BTreeSet::new();
    for (name, value) in overrides {
        if !value.is_finite() || !names.insert(name.to_ascii_lowercase()) {
            return Err(SpiceError::circuit(
                "nonfinite or duplicate DC source override",
            ));
        }
        let source = system
            .sources
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                SpiceError::circuit(format!("DC target {name} is not an independent source"))
            })?;
        for (row, sign) in &source.rows {
            target.add_to(*row, sign * (value - source.dc))?;
        }
    }
    let history = circuit.state_history();
    // Preserve exact linear solving (no nonlinear damping or continuation).
    if !circuit.devices().iter().any(|device| device.is_nonlinear()) {
        let values = system.a.solve(&target)?;
        let mut trial = history.trial();
        circuit.load(
            &LoadRequest {
                mode: AnalysisMode::OperatingPoint,
                solution: &values,
                model_context: context,
                integration: None,
                history: &history,
                forcing: None,
            },
            &mut SparseMatrix::new(n, n),
            &mut Vector::zeros(n),
            &mut trial,
        )?;
        return Ok(NewtonSolution {
            values,
            trial,
            iterations: 1,
        });
    }
    let branches = branch_rows(circuit);
    let stage = |guess: &Vector, scale: f64, gmin: f64| {
        newton::solve(guess, &branches, options, |x| {
            let mut a = SparseMatrix::new(n, n);
            let mut b = Vector::zeros(n);
            let mut trial = history.trial();
            circuit.load(
                &LoadRequest {
                    mode: AnalysisMode::OperatingPoint,
                    solution: x,
                    model_context: context,
                    integration: None,
                    history: &history,
                    forcing: None,
                },
                &mut a,
                &mut b,
                &mut trial,
            )?;
            for (row, branch) in branches.iter().enumerate() {
                b.add_to(
                    row,
                    scale * target.as_slice()[row] - original.as_slice()[row],
                )?;
                if !branch && gmin > 0. {
                    a.add(row, row, gmin)?;
                }
            }
            Ok((a, b, trial))
        })
    };
    let seed = initial.unwrap_or(&zero);
    let direct = stage(seed, 1., 0.);
    match direct {
        Ok(solved) => return Ok(solved),
        Err(ref error) if !matches!(error, SpiceError::Numerical { .. }) => return direct,
        _ => {}
    }
    let gmin_attempt = (|| {
        let mut guess = seed.clone();
        for gmin in [
            1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8, 1e-9, 1e-10, 1e-11, 1e-12,
        ] {
            guess = stage(&guess, 1., gmin)?.values;
        }
        stage(&guess, 1., 0.)
    })();
    if gmin_attempt.is_ok() {
        return gmin_attempt;
    }
    let mut guess = zero;
    for step in 0..=20 {
        guess = stage(&guess, f64::from(step) / 20., 1e-8)?.values;
    }
    stage(&guess, 1., 0.).map_err(|error| SpiceError::Numerical {
        context: "DC continuation".into(),
        message: format!("direct Newton, gmin and source stepping failed: {error}"),
    })
}
