//! Numerical conformance policy, separate from C rawfile byte-drift checks.
//!
//! Names/units follow `src/frontend/rawfile.c`; Rust MNA column order is not
//! ngspice's vector order. Bounds retain the production DC/AC regression policy.

use std::collections::BTreeMap;

use spice_analysis::{Plot, PlotFlags};

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
#[derive(Debug, Clone, Copy)]
pub(crate) struct TranTolerance {
    pub(crate) relative: f64,
    pub(crate) voltage_absolute: f64,
    pub(crate) current_absolute: f64,
}

#[cfg_attr(not(test), allow(dead_code))] // registered with the M3 transient gate
pub(crate) const TRAN: TranTolerance = TranTolerance {
    relative: 1e-3,
    voltage_absolute: 1e-6,
    current_absolute: 1e-12,
};

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
    use spice_analysis::Variable;
    use spice_core::Complex;

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
