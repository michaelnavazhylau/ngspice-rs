//! `txlload.c`/`txlacct.c`: recursive convolution over accepted V/I history.
//! C keeps two mutable TXLine snapshots and advances one on the next load.
//! Here each accepted sample carries the snapshot; trials are pure and a
//! rejected trial cannot change convolutions, interpolation cursors or history.
use super::Txl;
use super::pade::Term;
use crate::devices::delay::{DelayContext, DelayHistory, DelayLine, DelayUpdate};
use crate::primitives::{Complex, Real, SpiceError, SpiceResult};
#[cfg(test)]
mod tests;

const CLAMPED: usize = 30;
const WIDTH: usize = 31;
const H1: usize = 6;
const H2: usize = 12;
const H3: usize = 18;
fn error(message: &str) -> SpiceError {
    SpiceError::Numerical {
        context: "TXL txlload.c".into(),
        message: message.into(),
    }
}
/// C uses signed integer picoseconds for its history, truncating toward zero.
/// Reject out-of-range time rather than C's undefined float-to-int overflow.
fn ticks(time: Real) -> SpiceResult<Real> {
    let t = time * 1e12;
    if !t.is_finite() || t < 0. || t > f64::from(i32::MAX) {
        return Err(error(
            "time is outside C's signed picosecond history range (about 2.147 ms)",
        ));
    }
    Ok(t.trunc())
}
fn sample(h: &DelayHistory, i: usize) -> SpiceResult<[Real; WIDTH]> {
    let mut a = [0.; WIDTH];
    for (k, v) in a.iter_mut().enumerate() {
        *v = h
            .value(i, k)
            .ok_or_else(|| error("incomplete accepted history"))?;
    }
    Ok(a)
}
fn last(h: &DelayHistory) -> SpiceResult<[Real; WIDTH]> {
    sample(
        h,
        h.len()
            .checked_sub(1)
            .ok_or_else(|| error("empty accepted history"))?,
    )
}
/// `get_pvs_vi_txl`, with a read-only binary-search cursor instead of C's
/// linked-list cursor. Values are [input V, output V, input I, output I].
fn delayed(h: &DelayHistory, t: Real, previous: Real, now: Real) -> SpiceResult<([Real; 4], Real)> {
    let s = last(h)?;
    if t <= 0. {
        return Ok(([s[4], s[5], 0., 0.], 0.));
    }
    if t > previous {
        let ratio = (t - previous) / (now - previous);
        return Ok((
            [
                s[0] * (1. - ratio),
                s[1] * (1. - ratio),
                s[2] * (1. - ratio),
                s[3] * (1. - ratio),
            ],
            ratio,
        ));
    }
    let mut lo = 0;
    let mut hi = h.len() - 1;
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if ticks(h.time(mid).ok_or_else(|| error("missing history time"))?)? <= t {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    if lo + 1 == h.len() {
        return Ok(([s[0], s[1], s[2], s[3]], 0.));
    }
    let a = sample(h, lo)?;
    let b = sample(h, lo + 1)?;
    let t0 = ticks(h.time(lo).ok_or_else(|| error("missing history time"))?)?;
    let t1 = ticks(
        h.time(lo + 1)
            .ok_or_else(|| error("missing history time"))?,
    )?;
    if t1 <= t0 {
        return Err(error("non-increasing integer-picosecond history"));
    }
    let f = (t - t0) / (t1 - t0);
    Ok((std::array::from_fn(|k| a[k] + f * (b[k] - a[k])), 0.))
}
fn exp_pair(x: Real, y: Real, h: Real) -> Complex {
    let e = (x * h).exp();
    Complex::new(e * (y * h).cos(), e * (y * h).sin())
}
/// `right_consts_txl`: trapezoidal recurrence, including C's compressed
/// representation of the complex conjugate pair.
fn convolve(
    terms: &[Term],
    old: &[Real],
    out: &mut [Real],
    v1: [Real; 2],
    v2: [Real; 2],
    h: Real,
    pair: bool,
) -> [Real; 2] {
    let n = terms.len();
    let real = if pair { n - 2 } else { n };
    let half = 0.5 * h;
    let mut sums = [0.; 2];
    for i in 0..real {
        let t = terms[i];
        let e = (t.x * h).exp();
        for k in 0..2 {
            out[2 * i + k] = old[2 * i + k] * e + half * t.c * (v1[k] * e + v2[k]);
            sums[k] += out[2 * i + k];
        }
    }
    if pair {
        let a = n - 2;
        let b = n - 1;
        let e = exp_pair(terms[a].x, terms[b].x, h);
        let c = Complex::new(half * terms[a].c, half * terms[b].c);
        for k in 0..2 {
            let z = Complex::new(old[2 * a + k], old[2 * b + k]) * e
                + c * Complex::new(v1[k] * e.re + v2[k], v1[k] * e.im);
            out[2 * a + k] = z.re;
            out[2 * b + k] = z.im;
            sums[k] += 2. * z.re;
        }
    }
    sums
}
impl Txl {
    /// The future convolution state and branch RHS at a trial point.
    fn trial(
        &self,
        history: &DelayHistory,
        time: Real,
        dt: Real,
    ) -> SpiceResult<([Real; WIDTH], [Real; 2], Real)> {
        let old = last(history)?;
        let mut state = old;
        let c = &self.coefficients;
        let now = ticks(time)?;
        let previous = ticks((time - dt).max(0.))?.max(ticks(
            history.last_time().ok_or_else(|| error("empty history"))?,
        )?);
        let (v1, _) = delayed(history, previous - c.delay, previous, now)?;
        let (v2, ratio) = delayed(history, now - c.delay, previous, now)?;
        let mut rhs = [0.; 2];
        if !c.lossless {
            for (i, t) in c.h1.iter().enumerate() {
                let e = (t.x * dt).exp();
                for k in 0..2 {
                    rhs[k] -= old[H1 + 2 * i + k] * e + 0.5 * dt * t.c * e * old[k];
                }
            }
            let h3 = convolve(
                &c.h3,
                &old[H3..CLAMPED],
                &mut state[H3..CLAMPED],
                [v1[0], v1[1]],
                [v2[0], v2[1]],
                dt,
                c.complex,
            );
            let h2 = convolve(
                &c.h2,
                &old[H2..H3],
                &mut state[H2..H3],
                [v1[2], v1[3]],
                [v2[2], v2[3]],
                dt,
                c.complex,
            );
            rhs[0] += h3[1] + h2[1];
            rhs[1] += h3[0] + h2[0];
        }
        rhs[0] += c.atten3 * v2[1] + c.atten2 * v2[3];
        rhs[1] += c.atten3 * v2[0] + c.atten2 * v2[2];
        if state.iter().chain(&rhs).any(|v| !v.is_finite()) {
            return Err(error("nonfinite convolution"));
        }
        Ok((state, rhs, ratio))
    }
    pub(super) fn transient_stamp(
        &self,
        history: &DelayHistory,
        time: Real,
        dt: Real,
        gmin: Real,
    ) -> SpiceResult<([Real; 14], [Real; 2])> {
        let (_, rhs, ratio) = self.trial(history, time, dt)?;
        let c = &self.coefficients;
        let y = c.admit + 0.5 * dt * c.h1c;
        let v = ratio * (c.atten3 + 0.5 * dt * c.h3.iter().map(|t| t.c).sum::<Real>());
        let i = ratio * (c.atten2 + 0.5 * dt * c.h2.iter().map(|t| t.c).sum::<Real>());
        let g = 0.1 * gmin;
        Ok(([g, g, g, g, y, y, 1., 1., -1., -1., -v, -v, -i, -i], rhs))
    }
    fn values(&self, ctx: &DelayContext<'_>) -> SpiceResult<[Real; 4]> {
        let a = ctx.branches.start;
        if ctx.branches.len() < 2 {
            return Err(error("missing branch-current binding"));
        }
        Ok([
            ctx.node_voltage(self.terminals[0]),
            ctx.node_voltage(self.terminals[1]),
            ctx.solution
                .get(a)
                .ok_or_else(|| error("missing input current"))?,
            ctx.solution
                .get(a + 1)
                .ok_or_else(|| error("missing output current"))?,
        ])
    }
}
impl DelayLine for Txl {
    fn width(&self) -> usize {
        WIDTH
    }
    fn start(&self, ctx: &DelayContext<'_>) -> SpiceResult<DelayUpdate> {
        if ctx.initial_conditions {
            return Err(self.unsupported("transient uic (TXL has no initial-condition setters)"));
        }
        let v = self.values(ctx)?;
        let mut state = [0.; WIDTH];
        state[..4].copy_from_slice(&v);
        state[4] = v[0];
        state[5] = v[1];
        if !self.coefficients.lossless {
            for (base, terms) in [
                (H1, self.coefficients.h1.as_slice()),
                (H3, self.coefficients.h3.as_slice()),
            ] {
                for (i, t) in terms.iter().enumerate() {
                    state[base + 2 * i] = -v[0] * t.c / t.x;
                    state[base + 2 * i + 1] = -v[1] * t.c / t.x;
                }
            }
        }
        Ok(DelayUpdate {
            reset: Some(vec![(0., state.to_vec())]),
            ..DelayUpdate::default()
        })
    }
    fn accept(&self, history: &DelayHistory, ctx: &DelayContext<'_>) -> SpiceResult<DelayUpdate> {
        let old = last(history)?;
        let before = ticks(history.last_time().ok_or_else(|| error("empty history"))?)?;
        let now = ticks(ctx.time)?;
        // C adds V/I records only when the integer history time advances.
        if now <= before {
            return Ok(DelayUpdate::default());
        }
        let dt = ctx.steps[0];
        let (mut state, _, ratio) = self.trial(history, ctx.time, dt)?;
        let values = self.values(ctx)?;
        if !self.coefficients.lossless {
            // update_cnv_txl: C deliberately reuses (and rescales) bi/bo in
            // each loop iteration. Retain that order rather than substituting
            // the analytically exact, but different, integration formula.
            let delta = now - before;
            let mut slopes = [(values[0] - old[0]) / delta, (values[1] - old[1]) / delta];
            for (i, t) in self.coefficients.h1.iter().enumerate() {
                let e = (t.x * dt).exp();
                let factor = t.c / t.x;
                for k in 0..2 {
                    slopes[k] *= factor;
                    state[H1 + 2 * i + k] = (old[H1 + 2 * i + k] - slopes[k] * delta) * e
                        + (e - 1.) * (values[k] * factor + 1e12 * slopes[k] / t.x);
                }
            }
            if ratio > 0. {
                // update_delayed_cnv_txl's correction for the unknown part
                // of the delayed sample that the trial stamped in the matrix.
                for (base, terms, offset) in [
                    (H2, self.coefficients.h2.as_slice(), 2),
                    (H3, self.coefficients.h3.as_slice(), 0),
                ] {
                    for (i, t) in terms.iter().enumerate() {
                        for k in 0..2 {
                            state[base + 2 * i + k] +=
                                delta * 0.5e-12 * ratio * values[offset + k] * t.c;
                        }
                    }
                }
            }
        }
        // CKTmaxStep stays clamped even after the triggering gap is pruned.
        if dt > self.coefficients.delay * 1e-12 {
            state[CLAMPED] = 1.;
        }
        state[..4].copy_from_slice(&values);
        if state.iter().any(|v| !v.is_finite()) {
            return Err(error("nonfinite accepted convolution"));
        }
        // Keep the interpolation bracket before the next delayed time. DC
        // prehistory is carried explicitly in slots 4/5 after pruning.
        let cutoff = now - self.coefficients.delay;
        let mut drop = 0;
        while drop + 1 < history.len()
            && ticks(
                history
                    .time(drop + 1)
                    .ok_or_else(|| error("missing history time"))?,
            )? < cutoff
        {
            drop += 1;
        }
        Ok(DelayUpdate {
            drop_front: drop,
            append: Some((ctx.time, state.to_vec())),
            ..DelayUpdate::default()
        })
    }
    /// `TXLload`'s step control: when a load's `h` exceeds `taul`, C prints
    /// "your time step is too large for TXL tau." and sets
    /// `CKTmaxStep = 0.9 * taul`, which then governs the *following* steps.
    /// The first oversized step is therefore taken in C. Reproducing that
    /// ordering matters (it decides whether the run has one extra point), so
    /// the bound is offered only once an accepted sample gap already exceeded
    /// `taul`; before that the trial passes through unclamped.
    fn timestep_limit(
        &self,
        history: &DelayHistory,
        _ctx: &DelayContext<'_>,
    ) -> SpiceResult<Option<Real>> {
        if self.coefficients.lossless {
            return Ok(None);
        }
        let taul = self.coefficients.delay * 1e-12;
        Ok((last(history)?[CLAMPED] != 0.).then_some(0.9 * taul))
    }
}
