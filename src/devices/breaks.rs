//! The transient breakpoint table and the independent sources' accept-time
//! breakpoint scheduling.
//!
//! Upstream behavior: `src/spicelib/analysis/cktsetbk.c` (`CKTsetBreak`),
//! `cktclrbk.c` (`CKTclrBreak`), `src/spicelib/devices/vsrc/vsrcacct.c`
//! (`VSRCaccept`) and `isrc/isrcacct.c` (`ISRCaccept`). C does not precompute a
//! source's corners: at every accepted time point (`CKTaccept`) each source
//! requests at most its *next* corner, computed from the accepted time with
//! C's floating-point operation order, and `CKTsetBreak` merges the request
//! into a sorted table. The transient driver lands on (and equalises toward)
//! the table's first entry, so the exact breakpoint values decide the accepted
//! time points; this module reproduces them bit for bit.
//!
//! Like C (`vsrcacct.c`), SIN, EXP, SFFM and AM request no breakpoints; PULSE and
//! PWL do. Voltage and current PULSE sources schedule differently, as in C:
//! `VSRCaccept` requests the next corner after `time + CKTminBreak` whenever the
//! accepted time passed its last request, while `ISRCaccept` keeps the older
//! scheme that requests the following corner only when the accepted point is a
//! breakpoint (`CKTbreak`) lying within `1e-7 * PW` of a corner. The port's
//! [`Waveform::Step`] (no netlist spelling) schedules like a one-knot PWL.
//! ngspice's `xs` compatibility mode (PULSE phase in degrees) is not modelled,
//! matching [`crate::devices::Pulse`].

use crate::devices::{LinearSystem, SourceKind, TransientTiming, Waveform};
use crate::primitives::{Real, SpiceError, SpiceResult, almost_equal_ulps};

/// C's breakpoint table `CKTbreaks`: sorted times, always at least two
/// entries, the last of which is the final time until the run passes it.
#[derive(Debug, Clone, PartialEq)]
pub struct BreakTable {
    breaks: Vec<Real>,
    min_break: Real,
    final_time: Real,
}

impl BreakTable {
    /// `dctran.c`'s initial table `{0, CKTfinalTime}` with merge distance
    /// `min_break` (`CKTminBreak`).
    #[must_use]
    pub fn new(final_time: Real, min_break: Real) -> Self {
        Self {
            breaks: vec![0., final_time],
            min_break,
            final_time,
        }
    }

    /// `CKTminBreak`.
    #[must_use]
    pub const fn min_break(&self) -> Real {
        self.min_break
    }

    /// `CKTbreaks[0]`, the next breakpoint.
    #[must_use]
    pub fn first(&self) -> Real {
        self.breaks[0]
    }

    /// `CKTbreaks[1]`, the breakpoint after the next.
    #[must_use]
    pub fn second(&self) -> Real {
        self.breaks[1]
    }

    /// The whole table, ascending.
    #[must_use]
    pub fn times(&self) -> &[Real] {
        &self.breaks
    }

    /// `CKTsetBreak(ckt, time)` with `CKTtime = now`.
    ///
    /// A request within 3 ULPs of `now` is ignored. Requests within
    /// `CKTminBreak` of an existing entry merge into the earlier of the two.
    ///
    /// # Errors
    /// `time` is in the past (C: "breakpoint in the past - HELP!") or not finite.
    pub fn set(&mut self, now: Real, time: Real) -> SpiceResult<()> {
        if almost_equal_ulps(time, now, 3) {
            return Ok(());
        }
        if now > time || !time.is_finite() {
            return Err(SpiceError::Numerical {
                context: "transient".to_owned(),
                message: format!("breakpoint {time:e} in the past (time {now:e})"),
            });
        }
        for i in 0..self.breaks.len() {
            if self.breaks[i] > time {
                if self.breaks[i] - time <= self.min_break {
                    // Very close together: take the earlier point.
                    self.breaks[i] = time;
                    return Ok(());
                }
                if i > 0 && time - self.breaks[i - 1] <= self.min_break {
                    // Very close together, but after: skip.
                    return Ok(());
                }
                self.breaks.insert(i, time);
                return Ok(());
            }
        }
        if let Some(last) = self.breaks.last()
            && time - last <= self.min_break
        {
            return Ok(());
        }
        self.breaks.push(time);
        Ok(())
    }

    /// `CKTclrBreak`: drops the first entry; a two-entry table becomes
    /// `{CKTbreaks[1], CKTfinalTime}`.
    pub fn clear(&mut self) {
        if self.breaks.len() > 2 {
            self.breaks.remove(0);
        } else {
            self.breaks[0] = self.breaks[1];
            self.breaks[1] = self.final_time;
        }
    }
}

/// Per-source accept-time breakpoint state (`VSRCbreak_time`/`ISRCbreak_time`)
/// of a [`LinearSystem`]'s independent sources, in system order.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceBreaks {
    break_time: Vec<Real>,
}

impl SourceBreaks {
    /// The state at the start of a run: `vsrcset.c` sets `VSRCbreak_time =
    /// -1`; `ISRCbreak_time` is never initialized and starts at zero
    /// (`calloc`). Both request their first corner at `t = 0`.
    #[must_use]
    pub fn new(system: &LinearSystem) -> Self {
        Self {
            break_time: system
                .sources
                .iter()
                .map(|source| match source.kind {
                    SourceKind::Voltage => -1.,
                    SourceKind::Current => 0.,
                })
                .collect(),
        }
    }

    /// The sources' `DEVaccept` breakpoint requests at the accepted time `now`
    /// (`CKTtime`). `at_break` is C's `CKTbreak` (the accepted step was cut to
    /// land on a breakpoint, or `now` is the initial point).
    ///
    /// # Errors
    /// A request in the past, or unresolved PULSE/function defaults.
    pub fn accept(
        &mut self,
        system: &LinearSystem,
        now: Real,
        at_break: bool,
        timing: &TransientTiming,
        table: &mut BreakTable,
    ) -> SpiceResult<()> {
        for (source, break_time) in system.sources.iter().zip(&mut self.break_time) {
            let request = Request {
                now,
                at_break,
                timing,
                kind: source.kind,
            };
            request.waveform(&source.waveform, break_time, table)?;
        }
        Ok(())
    }
}

struct Request<'a> {
    now: Real,
    at_break: bool,
    timing: &'a TransientTiming,
    kind: SourceKind,
}

impl Request<'_> {
    fn waveform(
        &self,
        waveform: &Waveform,
        break_time: &mut Real,
        table: &mut BreakTable,
    ) -> SpiceResult<()> {
        match waveform {
            Waveform::Constant(_) | Waveform::Function(_) => Ok(()),
            Waveform::PortSeries { baseline, .. } => self.waveform(baseline, break_time, table),
            Waveform::Pulse(pulse) => match self.kind {
                SourceKind::Voltage => self.vsrc_pulse(pulse, break_time, table),
                SourceKind::Current => self.isrc_pulse(pulse, table),
            },
            Waveform::Pwl(knots) => {
                let times: Vec<Real> = knots.iter().map(|(t, _)| *t).collect();
                self.pwl(&times, 0., None, break_time, table)
            }
            Waveform::PwlSource(pwl) => {
                let times: Vec<Real> = pwl.knots().iter().map(|(t, _)| *t).collect();
                self.pwl(&times, pwl.delay(), pwl.repeat(), break_time, table)
            }
            Waveform::Step { time, .. } => self.pwl(&[*time], 0., None, break_time, table),
            Waveform::PulseDefaults(_) | Waveform::FunctionDefaults(_) => Err(SpiceError::circuit(
                "source defaults are unresolved; bind transient timing before scheduling breakpoints",
            )),
        }
    }

    /// `VSRCaccept`, `case PULSE` (default compatibility mode).
    fn vsrc_pulse(
        &self,
        pulse: &crate::devices::Pulse,
        break_time: &mut Real,
        table: &mut BreakTable,
    ) -> SpiceResult<()> {
        let now = self.now;
        let (td, tr, tf, pw, per) = (
            pulse.delay(),
            pulse.rise(),
            pulse.fall(),
            pulse.width(),
            pulse.period(),
        );
        // Offset time by delay.
        let mut time = now - td;
        if let Some(tmax) = pulse.stop()
            && time > tmax
        {
            return Ok(());
        }
        if now >= *break_time {
            if time >= per {
                // Repeating signal: where in period are we?
                let basetime = per * (time / per).floor();
                time -= basetime;
            }
            // A request very close to the current time would be ignored:
            // adjust so the next corner is selected.
            let min_break = table.min_break();
            let atime = time + min_break;
            let wait = if atime < 0. {
                -time
            } else if atime < tr {
                tr - time
            } else if atime < tr + pw {
                tr + pw - time
            } else if atime < tr + pw + tf {
                tr + pw + tf - time
            } else {
                per - time
            };
            *break_time = now + wait;
            table.set(now, *break_time)?;
            // A timestep ending just before the break time may have the
            // request ignored: lower the threshold for the following one.
            *break_time -= min_break;
        }
        Ok(())
    }

    /// `ISRCaccept`, `case PULSE` (default compatibility mode): the
    /// `CKTbreak`/`SAMETIME` scheme.
    fn isrc_pulse(&self, pulse: &crate::devices::Pulse, table: &mut BreakTable) -> SpiceResult<()> {
        let now = self.now;
        let (td, tr, tf, pw, per) = (
            pulse.delay(),
            pulse.rise(),
            pulse.fall(),
            pulse.width(),
            pulse.period(),
        );
        // #define SAMETIME(a,b) (fabs((a)-(b)) <= TIMETOL * PW), TIMETOL 1e-7
        let same = |a: Real, b: Real| (a - b).abs() <= 1e-7 * pw;
        let mut time = now - td;
        let tshift = td;
        let mut basetime = 0.;
        if let Some(tmax) = pulse.stop()
            && time > tmax
        {
            return Ok(());
        }
        if time >= per {
            basetime = per * (time / per).floor();
            time -= basetime;
        }
        if !self.at_break {
            return Ok(());
        }
        let request = if time <= 0. || time >= tr + pw + tf {
            if same(time, 0.) {
                Some(basetime + tr + tshift)
            } else if same(tr + pw + tf, time) {
                Some(basetime + per + tshift)
            } else if time == -tshift {
                Some(basetime + tshift)
            } else if same(per, time) {
                Some(basetime + tshift + tr + per)
            } else {
                None
            }
        } else if time >= tr && time <= tr + pw {
            if same(time, tr) {
                Some(basetime + tshift + tr + pw)
            } else if same(tr + pw, time) {
                Some(basetime + tshift + tr + pw + tf)
            } else {
                None
            }
        } else if time > 0. && time < tr {
            if same(time, 0.) {
                Some(basetime + tshift + tr)
            } else if same(time, tr) {
                Some(basetime + tshift + tr + pw)
            } else {
                None
            }
        } else if same(time, tr + pw) {
            // time > TR + PW && < TR + PW + TF
            Some(basetime + tshift + tr + pw + tf)
        } else if same(time, tr + pw + tf) {
            Some(basetime + tshift + per)
        } else {
            None
        };
        match request {
            Some(time) => table.set(now, time),
            None => Ok(()),
        }
    }

    /// `VSRCaccept`/`ISRCaccept`, `case PWL`: knot times `times` (as given),
    /// `td=` delay and the `r=` restart knot index.
    fn pwl(
        &self,
        times: &[Real],
        delay: Real,
        repeat: Option<usize>,
        break_time: &mut Real,
        table: &mut BreakTable,
    ) -> SpiceResult<()> {
        let now = self.now;
        if now < *break_time {
            return Ok(());
        }
        let mut time = now - delay;
        let Some(&end) = times.last() else {
            return Ok(());
        };
        if time > end {
            match repeat {
                Some(index) => {
                    let start = times[index];
                    let period = end - start;
                    time -= start;
                    time -= period * (time / period).floor();
                    time += start;
                }
                None => {
                    *break_time = self.timing.final_time();
                    return Ok(());
                }
            }
        }
        let min_break = table.min_break();
        let atime = time + min_break;
        if let Some(knot) = times.iter().find(|knot| **knot > atime) {
            *break_time = now + knot - time;
            table.set(now, *break_time)?;
            *break_time -= min_break;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_break_merges_like_cktsetbk() {
        let mut table = BreakTable::new(10., 0.1);
        table.set(0., 5.).unwrap();
        assert_eq!(table.times(), &[0., 5., 10.]);
        // Within min_break before an entry: the earlier time replaces it.
        table.set(0., 4.95).unwrap();
        assert_eq!(table.times(), &[0., 4.95, 10.]);
        // Within min_break after an entry: skipped.
        table.set(0., 5.0).unwrap();
        assert_eq!(table.times(), &[0., 4.95, 10.]);
        // Beyond the end, close to the last entry: skipped; farther: appended.
        table.set(0., 10.05).unwrap();
        assert_eq!(table.times(), &[0., 4.95, 10.]);
        table.set(0., 12.).unwrap();
        assert_eq!(table.times(), &[0., 4.95, 10., 12.]);
        // A request at the current time is ignored, one in the past fails.
        table.set(4., 4.).unwrap();
        assert_eq!(table.times().len(), 4);
        assert!(table.set(4., 3.).is_err());
    }

    #[test]
    fn clear_break_keeps_two_entries() {
        let mut table = BreakTable::new(10., 0.1);
        table.set(0., 5.).unwrap();
        table.clear();
        assert_eq!(table.times(), &[5., 10.]);
        table.clear();
        assert_eq!(table.times(), &[10., 10.]);
    }
}
