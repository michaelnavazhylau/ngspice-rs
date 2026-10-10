//! Deterministic line-printer plots. C: `frontend/plotting/agraf.c::ft_agraf`.
//! Like C, the independent axis runs down the page and dependent values across
//! it. Pagination, date headers and terminal-dependent widths are deliberately
//! replaced by a fixed 60-column field; no graphical backend is involved.
use crate::analysis::Plot;
use crate::primitives::{SpiceError, SpiceResult};
use std::fmt::Write as _;

/// Render selected vectors, using C's legend and collision characters.
/// Complex vectors are plotted by their real component (C `ft_agraf`).
///
/// # Errors
/// Invalid, empty or nonmonotonic data, nonfinite scale arithmetic.
pub fn render(plot: &Plot) -> SpiceResult<String> {
    let invalid = |message: &str| SpiceError::Unsupported {
        feature: format!(".plot: {message}"),
        location: None,
    };
    if plot.variables.len() < 2 || plot.points.is_empty() {
        return Err(invalid("needs a scale and at least one dependent vector"));
    }
    if plot.points.windows(2).any(|p| p[1][0].re < p[0][0].re) {
        return Err(invalid("scale must be nondecreasing"));
    }
    if plot
        .points
        .iter()
        .any(|row| row.len() != plot.variables.len() || row.iter().any(|v| !v.is_finite()))
    {
        return Err(invalid("inconsistent or nonfinite plot values"));
    }
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for row in &plot.points {
        for value in &row[1..] {
            low = low.min(value.re);
            high = high.max(value.re);
        }
    }
    if low == high {
        let delta = low.abs().max(1.) * 0.1;
        low -= delta;
        high += delta;
    }
    let span = high - low;
    if !span.is_finite() || span <= 0. {
        return Err(invalid("dependent range overflows"));
    }
    const SYMBOLS: &[u8] = b"+*=$%!0123456789";
    let mut text = format!("plot: {}\nLegend:  ", plot.plotname);
    for (i, variable) in plot.variables[1..].iter().enumerate() {
        let _ = write!(
            text,
            "{} = {}  ",
            char::from(*SYMBOLS.get(i).unwrap_or(&b'#')),
            variable.name
        );
    }
    let _ = writeln!(
        text,
        "\n{}     {:.3e} .. {:.3e}",
        plot.variables[0].name, low, high
    );
    for row in &plot.points {
        let mut field = [b' '; 61];
        for i in (0..=60).step_by(15) {
            field[i] = b'.';
        }
        for (i, value) in row[1..].iter().enumerate() {
            let at = (((value.re - low) / span) * 60.).round().clamp(0., 60.) as usize;
            field[at] = if matches!(field[at], b' ' | b'.') {
                *SYMBOLS.get(i).unwrap_or(&b'#')
            } else {
                b'X'
            };
        }
        let _ = writeln!(
            text,
            "{:.3e} {:.3e} {}",
            row[0].re,
            row[1].re,
            String::from_utf8_lossy(&field)
        );
    }
    Ok(text)
}
