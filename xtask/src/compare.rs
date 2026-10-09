//! Numerical conformance policy, separate from C rawfile byte-drift checks.
//!
//! Names/units follow `src/frontend/rawfile.c`; Rust MNA column order is not
//! ngspice's vector order. Bounds retain the production DC/AC regression policy.

use std::collections::BTreeMap;

use ngspice_rs::analysis::{Plot, PlotFlags};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Tolerance {
    relative: f64,
    absolute: f64,
}

pub(crate) const DC: Tolerance = Tolerance {
    relative: 1e-12,
    absolute: 1e-15,
};
pub(crate) const AC: Tolerance = Tolerance {
    relative: 1e-10,
    absolute: 1e-12,
};

/// Nonlinear bias/AC allow 1 ppm for independently converged bias Jacobians,
/// plus a 1 p-unit floor. The linear LU-only tolerances remain unchanged.
pub(crate) const NONLINEAR: Tolerance = Tolerance {
    relative: 1e-6,
    absolute: 1e-12,
};

/// `.noise` (#100), linear circuits: the AC bound relative to each value, with
/// an absolute floor scaled to noise rather than to node voltages. Densities
/// are about 1e-9 V/sqrt(Hz) and integrated totals 1e-7 V and up, so a 1e-12
/// floor would hide whole generators; 1e-20 sits far below any physical
/// density yet above the residue C's adjoint leaves for a generator whose
/// transfer is exactly zero (e.g. about 2e-25 V/sqrt(Hz) for a resistor in
/// series with the current-source input).
pub(crate) const NOISE: Tolerance = Tolerance {
    relative: 1e-10,
    absolute: 1e-20,
};

/// `.noise` of nonlinear circuits: the nonlinear 1 ppm bias bound (the
/// generators are evaluated at independently converged operating points)
/// with the noise floor of [`NOISE`].
pub(crate) const NOISE_NONLINEAR: Tolerance = Tolerance {
    relative: 1e-6,
    absolute: 1e-20,
};

/// The original diode sweep was captured at C's default nonlinear RELTOL
/// (1e-3), including bypass. Independent junction/KCL checks establish that the
/// more accurate Rust root, not its equations, causes the 0.062% last-point
/// current difference. This bound applies only to that legacy golden; newly
/// demonstrated DC/AC decks retain NONLINEAR's 1 ppm requirement.
pub(crate) const LEGACY_DIODE_DC: Tolerance = Tolerance {
    relative: 1e-3,
    absolute: 1e-12,
};

/// Transient bounds, by signal kind. Transient waveforms are compared at
/// shared physical times (see `tran.rs`), never step by step, so the bound must
/// cover two different integrators plus linear resampling of the denser side:
/// `|Rust - C| <= relative*|C| + absolute(kind)`.
///
/// * `relative = 1e-3` is ngspice's default `reltol`, the accuracy its own
///   local-truncation control targets per step; two independent solutions of
///   the same circuit can legitimately differ by about that much.
/// * `voltage_absolute = 1e-6` V is ngspice's default `vntol` (1 uV), the
///   natural voltage floor near zero crossings.
/// * `current_absolute = 1e-12` A is ngspice's default `abstol` (1 pA), the
///   natural current floor for the small branch currents of the linear decks.
///
/// These are the simulator's own default accuracy floors, not values fitted to
/// any fixture. Tighten only with evidence; never loosen to make a case pass.
///
/// `peak_relative` adds `peak_relative * max|C signal|` to the bound (0 for
/// [`TRAN`]); see [`TRAN_RESTART`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct TranTolerance {
    pub(crate) peak_relative: f64,
    pub(crate) relative: f64,
    pub(crate) voltage_absolute: f64,
    pub(crate) current_absolute: f64,
}

pub(crate) const TRAN: TranTolerance = TranTolerance {
    peak_relative: 0.0,
    relative: 1e-3,
    voltage_absolute: 1e-6,
    current_absolute: 1e-12,
};

/// Bound for an *independent, more accurate* Rust integrator (explicit diffsol
/// BDF, rtol 1e-7) against the C trapezoidal reference on decks with source
/// corners: `|Rust - C| <= 1e-3 |C| + 1e-3 max|C| + floor`.
///
/// C restarts its integrator with a backward-Euler step at every breakpoint
/// (`dctran.c`, order 1 after a break). That step's local error is first order
/// in the step and is visible against an exact solution right after a corner
/// (measured against closed forms and an independent RK4 integration: for
/// example a 4% error of `v(out)` 10 us after a PWL corner, about 2e-5 V on a
/// 1 V signal). It is not a Rust defect: the companion driver reproduces it
/// (worst error 0.000 of `TRAN`) and BDF agrees with the analytic response.
/// ngspice's own truncation control only bounds per-step charge error to
/// `trtol * (reltol * max|q| + chgtol)`, i.e. relative to the *peak* charge
/// scale, never to the instantaneous value, so a purely pointwise relative
/// bound (`TRAN`) is stricter than C guarantees for small values. This policy
/// adds `reltol` (1e-3, ngspice default, not fitted) times the signal peak.
/// It applies only to Rust-only backend variants, never to the C-parity
/// companion run; BDF accuracy itself is established by the tighter analytic
/// tests in `tests/`.
pub(crate) const TRAN_RESTART: TranTolerance = TranTolerance {
    peak_relative: 1e-3,
    ..TRAN
};

/// Pole-zero bound: each Rust root `r` matched to a C root `c` of the same
/// kind (pole or zero) must satisfy
/// `|r - c| <= relative * |c| + scale_relative * max|C root of the plot|`.
///
/// * `relative = 1e-6` is the relative tolerance C's own root search accepts
///   when it decides that a trial coincides with a found root
///   (`CKTpzRunTrial`, `cktpzstr.c`: `reltol = 1e-6` for `ISAROOT`), so C's
///   roots are not claimed to be more accurate than that. It also covers the
///   `~sqrt(eps)` split of an exact double root that any eigenvalue method
///   (and C's deflated Muller search) reports.
/// * `scale_relative = 1e-9` of the plot's largest C root (poles and zeros
///   together: the circuit's frequency scale) is the floor for roots at or
///   near the origin: C reports such a root as exactly zero (its determinant
///   LU is singular there), the port's eigenvalue is accurate to rounding of
///   the pencil's norm, which the largest root measures.
///
/// Roots are matched as unordered multisets (the order of equal real parts
/// and of perturbed multiple roots is not meaningful); never loosen these to
/// absorb a missing or extra root, which is a count mismatch.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RootTolerance {
    relative: f64,
    scale_relative: f64,
}

pub(crate) const POLE_ZERO: RootTolerance = RootTolerance {
    relative: 1e-6,
    scale_relative: 1e-9,
};

/// The roots of one kind (`pole` or `zero`) of a pole-zero plot, in column
/// order.
fn root_set(plot: &Plot, kind: &str) -> Vec<(String, ngspice_rs::primitives::Complex)> {
    let prefix = format!("v({kind}(");
    plot.variables
        .iter()
        .enumerate()
        .filter(|(_, variable)| variable.name.to_ascii_lowercase().starts_with(&prefix))
        .map(|(index, variable)| (variable.name.clone(), plot.points[0][index]))
        .collect()
}

/// Compares a Rust pole-zero plot with a C one as two unordered root sets
/// (poles, zeros) under [`RootTolerance`]. The plot structure (name, flags,
/// variable names, units and real flags) must match exactly, so the root
/// counts must too. Returns a one-line summary.
pub(crate) fn roots(got: &Plot, want: &Plot, tolerance: RootTolerance) -> Result<String, String> {
    let got_columns = validate(got, "Rust")?;
    let want_columns = validate(want, "C")?;
    check_structure(got, want, &got_columns, &want_columns)?;
    if got.point_count() != 1 || want.point_count() != 1 {
        return Err(format!(
            "a pole-zero plot has one point: Rust {}, C {}",
            got.point_count(),
            want.point_count()
        ));
    }
    let mut worst_ratio: f64 = 0.0;
    let mut counts = Vec::new();
    let scale = want.points[0]
        .iter()
        .fold(0.0_f64, |m, z| m.max(z.magnitude()));
    for kind in ["pole", "zero"] {
        let (rust, c) = (root_set(got, kind), root_set(want, kind));
        if rust.len() != c.len() {
            return Err(format!(
                "{kind} count mismatch: Rust {}, C {}",
                rust.len(),
                c.len()
            ));
        }
        let bound = |c: ngspice_rs::primitives::Complex| {
            tolerance.relative * c.magnitude() + tolerance.scale_relative * scale
        };
        // Greedy assignment by error/bound ratio over every pair.
        let mut pairs = Vec::with_capacity(rust.len() * c.len());
        for (i, (_, r)) in rust.iter().enumerate() {
            for (j, (_, w)) in c.iter().enumerate() {
                pairs.push(((*r - *w).magnitude() / bound(*w), i, j));
            }
        }
        pairs.sort_by(|x, y| x.0.total_cmp(&y.0));
        let (mut rust_used, mut c_used) = (vec![false; rust.len()], vec![false; c.len()]);
        for (ratio, i, j) in pairs {
            if rust_used[i] || c_used[j] {
                continue;
            }
            rust_used[i] = true;
            c_used[j] = true;
            if ratio.is_nan() || ratio > 1.0 {
                return Err(format!(
                    "{kind} mismatch: C {} = {:.17e}{:+.17e}j has no Rust root within the bound \
                     (nearest unmatched Rust {} = {:.17e}{:+.17e}j, {ratio:.6e}x bound)",
                    c[j].0, c[j].1.re, c[j].1.im, rust[i].0, rust[i].1.re, rust[i].1.im
                ));
            }
            worst_ratio = worst_ratio.max(ratio);
        }
        counts.push(format!("{} {kind}(s)", c.len()));
    }
    Ok(format!(
        "{}, worst error {worst_ratio:.3e} of bound",
        counts.join(" + ")
    ))
}

pub(crate) fn validate(plot: &Plot, label: &str) -> Result<BTreeMap<String, usize>, String> {
    if plot.is_empty() {
        return Err(format!("{label}: empty plot"));
    }
    let mut columns = BTreeMap::new();
    for (index, variable) in plot.variables.iter().enumerate() {
        let name = variable.name.to_ascii_lowercase();
        if name.is_empty() || columns.insert(name.clone(), index).is_some() {
            return Err(format!("{label}: empty or duplicate variable '{name}'"));
        }
    }
    for (index, point) in plot.points.iter().enumerate() {
        if point.len() != plot.variable_count() {
            return Err(format!("{label}: point {index} has invalid row shape"));
        }
        for (variable, value) in plot.variables.iter().zip(point) {
            if !value.is_finite() {
                return Err(format!(
                    "{label}: point {index}, {} has nonfinite value",
                    variable.name
                ));
            }
            if (plot.flags == PlotFlags::Real || variable.is_real) && value.im != 0.0 {
                return Err(format!(
                    "{label}: point {index}, {} has imaginary data flagged real",
                    variable.name
                ));
            }
        }
    }
    Ok(columns)
}

/// Plot name/flags, variable-name set, units and real-vector flags must match
/// (shared by the point-wise and transient comparators).
pub(crate) fn check_structure(
    got: &Plot,
    want: &Plot,
    got_columns: &BTreeMap<String, usize>,
    want_columns: &BTreeMap<String, usize>,
) -> Result<(), String> {
    if got.plotname != want.plotname || got.flags != want.flags {
        return Err(format!(
            "plot metadata mismatch: Rust '{}'/{}; C '{}'/{}",
            got.plotname,
            got.flags.as_rawfile(),
            want.plotname,
            want.flags.as_rawfile()
        ));
    }
    let missing: Vec<_> = want_columns
        .keys()
        .filter(|name| !got_columns.contains_key(*name))
        .collect();
    let extra: Vec<_> = got_columns
        .keys()
        .filter(|name| !want_columns.contains_key(*name))
        .collect();
    if !missing.is_empty() || !extra.is_empty() {
        return Err(format!(
            "variable mismatch: missing {missing:?}; extra {extra:?}"
        ));
    }
    for (name, &want_index) in want_columns {
        let actual = &got.variables[got_columns[name]];
        let expected = &want.variables[want_index];
        if actual.unit != expected.unit || actual.is_real != expected.is_real {
            return Err(format!(
                "variable metadata mismatch for '{name}': Rust {actual:?}; C {expected:?}"
            ));
        }
    }
    Ok(())
}

/// Compare one production plot with one committed C plot. Header dates,
/// commands, titles and internal plot IDs are not numerical metadata.
/// Axis identity is explicit in the fixture registry, not inferred from order.
/// Complex components each satisfy `|got-want| <= relative*|want| + absolute`.
pub(crate) fn plots(
    got: &Plot,
    want: &Plot,
    tolerance: Tolerance,
    axis: Option<&str>,
) -> Result<(), String> {
    let got_columns = validate(got, "Rust")?;
    let want_columns = validate(want, "C")?;
    check_structure(got, want, &got_columns, &want_columns)?;
    if got.point_count() != want.point_count() {
        return Err(format!(
            "point count mismatch: Rust {}; C {}",
            got.point_count(),
            want.point_count()
        ));
    }
    if let Some(axis) = axis {
        for (plot, columns, label) in [(got, &got_columns, "Rust"), (want, &want_columns, "C")] {
            let &column = columns
                .get(axis)
                .ok_or_else(|| format!("{label}: missing axis '{axis}'"))?;
            let mut previous = None;
            for (index, row) in plot.points.iter().enumerate() {
                let value = row[column];
                if value.im != 0.0 || value.re <= 0.0 || previous.is_some_and(|p| p >= value.re) {
                    return Err(format!(
                        "{label}: invalid positive increasing axis '{axis}' at point {index}"
                    ));
                }
                previous = Some(value.re);
            }
        }
    }
    let mut first = None;
    let mut worst = None;
    let mut worst_ratio = 0.0;
    let mut mismatches = 0usize;
    // C column order supplies stable, human-readable first-mismatch diagnostics.
    for (point, (actual, expected)) in got.points.iter().zip(&want.points).enumerate() {
        for (column, variable) in want.variables.iter().enumerate() {
            let got = actual[got_columns[&variable.name.to_ascii_lowercase()]];
            let want = expected[column];
            for (component, got, want) in [("re", got.re, want.re), ("im", got.im, want.im)] {
                let error = (got - want).abs();
                let limit = tolerance.relative * want.abs() + tolerance.absolute;
                if error > limit {
                    mismatches += 1;
                    let report = format!(
                        "point {point}, {}.{component}: Rust {got:.17e}, C {want:.17e}, |error| {error:.6e} > bound {limit:.6e}",
                        variable.name
                    );
                    if first.is_none() {
                        first = Some(report.clone());
                    }
                    let ratio = error / limit;
                    if ratio > worst_ratio {
                        worst_ratio = ratio;
                        worst = Some(report);
                    }
                }
            }
        }
    }
    match first {
        Some(first) => Err(format!(
            "{mismatches} component mismatch(es)\nfirst: {first}\nworst ({worst_ratio:.6e}x bound): {}",
            worst.as_deref().unwrap_or(&first)
        )),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ngspice_rs::analysis::Variable;
    use ngspice_rs::primitives::Complex;

    fn plot() -> Plot {
        let mut plot = Plot::new("ac1", "AC Analysis", PlotFlags::Complex);
        plot.variables = vec![
            Variable::complex("frequency", "frequency"),
            Variable::complex("v(out)", "voltage"),
        ];
        plot.points = vec![
            vec![Complex::real(100.0), Complex::new(0.5, -0.25)],
            vec![Complex::real(1000.0), Complex::ZERO],
        ];
        plot
    }
    fn compare(got: &Plot, want: &Plot) -> Result<(), String> {
        plots(got, want, AC, Some("frequency"))
    }

    #[test]
    fn order_and_case_do_not_matter_but_metadata_does() {
        let want = plot();
        let mut got = want.clone();
        got.variables.swap(0, 1);
        got.variables[0].name = "V(OUT)".into();
        for row in &mut got.points {
            row.swap(0, 1);
        }
        got.name = "unrelated internal ID".into();
        compare(&got, &want).unwrap();
        got.variables[0].unit = "current".into();
        assert!(compare(&got, &want).unwrap_err().contains("metadata"));
        got = want.clone();
        got.variables[1].is_real = true;
        assert!(compare(&got, &want).is_err());
        got = want.clone();
        got.plotname = "Operating Point".into();
        assert!(compare(&got, &want).is_err());
        got = want.clone();
        got.flags = PlotFlags::Real;
        assert!(compare(&got, &want).is_err());
    }

    #[test]
    fn missing_extra_renamed_and_duplicate_columns_fail() {
        let want = plot();
        for name in ["renamed", "FREQUENCY", ""] {
            let mut got = want.clone();
            got.variables[1].name = name.into();
            assert!(compare(&got, &want).is_err());
        }
        let mut got = want.clone();
        got.variables.pop();
        for row in &mut got.points {
            row.pop();
        }
        assert!(compare(&got, &want).unwrap_err().contains("missing"));
        assert!(compare(&want, &got).unwrap_err().contains("extra"));
    }

    #[test]
    fn shape_empty_and_nonfinite_data_fail_on_either_side() {
        let want = plot();
        let mut cases = Vec::new();
        let mut bad = want.clone();
        bad.points[0].pop();
        cases.push(bad);
        let mut bad = want.clone();
        bad.points.pop();
        cases.push(bad);
        let mut bad = want.clone();
        bad.points.clear();
        cases.push(bad);
        let mut bad = want.clone();
        bad.variables.clear();
        cases.push(bad);
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for imaginary in [false, true] {
                let mut bad = want.clone();
                if imaginary {
                    bad.points[0][1].im = value;
                } else {
                    bad.points[0][1].re = value;
                }
                cases.push(bad);
            }
        }
        for bad in cases {
            assert!(compare(&bad, &want).is_err());
            assert!(compare(&want, &bad).is_err());
        }
    }

    #[test]
    fn components_and_zero_use_relative_plus_absolute_bounds() {
        for tolerance in [DC, AC] {
            let want = plot();
            for (point, component) in [(0, "re"), (0, "im"), (1, "re"), (1, "im")] {
                let mut got = want.clone();
                let value = if component == "re" {
                    &mut got.points[point][1].re
                } else {
                    &mut got.points[point][1].im
                };
                let bound = tolerance.relative * value.abs() + tolerance.absolute;
                *value += 0.5 * bound;
                plots(&got, &want, tolerance, None).unwrap();
                *if component == "re" {
                    &mut got.points[point][1].re
                } else {
                    &mut got.points[point][1].im
                } += 2.0 * bound;
                let error = plots(&got, &want, tolerance, None).unwrap_err();
                assert!(error.contains(&format!("v(out).{component}")), "{error}");
                assert!(error.contains("first:") && error.contains("worst"));
            }
        }
    }

    #[test]
    fn first_and_worst_mismatches_are_distinct() {
        let want = plot();
        let mut got = want.clone();
        got.points[0][1].re += 0.01;
        got.points[1][1].im = 1.0;
        let report = compare(&got, &want).unwrap_err();
        assert!(report.contains("first: point 0, v(out).re"), "{report}");
        assert!(report.contains("x bound): point 1, v(out).im"), "{report}");
    }

    fn pole_zero(poles: &[Complex], zeros: &[Complex]) -> Plot {
        let mut plot = Plot::new("pz1", "Pole-Zero Analysis", PlotFlags::Complex);
        for (index, _) in poles.iter().enumerate() {
            plot.variables.push(Variable::complex(
                format!("v(pole({}))", index + 1),
                "voltage",
            ));
        }
        for (index, _) in zeros.iter().enumerate() {
            plot.variables.push(Variable::complex(
                format!("v(zero({}))", index + 1),
                "voltage",
            ));
        }
        plot.points = vec![poles.iter().chain(zeros).copied().collect()];
        plot
    }

    #[test]
    fn root_sets_match_unordered_within_relative_and_scale_bounds() {
        let p = [
            Complex::new(-1e3, 2e3),
            Complex::new(-1e3, -2e3),
            Complex::real(-5e6),
        ];
        let want = pole_zero(&p, &[Complex::ZERO, Complex::real(-1e4)]);
        // Reordered, and within the bound: 1e-7 relative, and a zero at
        // 1e-6 (below 1e-9 * 5e6 = 5e-3) for C's exact zero.
        let got = pole_zero(
            &[p[2] * Complex::real(1. + 1e-7), p[1], p[0]],
            &[Complex::real(-1e4), Complex::real(1e-6)],
        );
        assert!(
            roots(&got, &want, POLE_ZERO)
                .unwrap()
                .contains("3 pole(s) + 2 zero(s)")
        );
        let far = pole_zero(&p, &[Complex::real(1e-2), Complex::real(-1e4)]);
        assert!(
            roots(&far, &want, POLE_ZERO)
                .unwrap_err()
                .contains("zero mismatch")
        );
        let moved = pole_zero(
            &[p[0], p[1], Complex::real(-5e6 * (1. + 1e-5))],
            &[Complex::ZERO, Complex::real(-1e4)],
        );
        assert!(
            roots(&moved, &want, POLE_ZERO)
                .unwrap_err()
                .contains("pole mismatch")
        );
        // A missing root is a structure (count) failure, never a tolerance one.
        let fewer = pole_zero(&p[..2], &[Complex::ZERO, Complex::real(-1e4)]);
        assert!(roots(&fewer, &want, POLE_ZERO).is_err());
        let mut renamed = want.clone();
        renamed.plotname = "AC Analysis".into();
        assert!(roots(&renamed, &want, POLE_ZERO).is_err());
    }

    #[test]
    fn axis_identity_values_and_monotonicity_are_checked() {
        let want = plot();
        assert!(plots(&want, &want, AC, Some("time")).is_err());
        for value in [Complex::ZERO, Complex::real(-1.0), Complex::new(100.0, 1.0)] {
            let mut got = want.clone();
            got.points[0][0] = value;
            assert!(compare(&got, &want).unwrap_err().contains("axis"));
        }
        let mut got = want.clone();
        got.points[1][0] = got.points[0][0];
        assert!(compare(&got, &want).unwrap_err().contains("axis"));
        got = want.clone();
        got.points[1][0].re += 1.0;
        assert!(compare(&got, &want).unwrap_err().contains("frequency.re"));
    }
}
