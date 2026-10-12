//! The device-driven delay-history and breakpoint infrastructure
//! (`devices::delay`, `Circuit::accept_transient_point`, the companion
//! driver's breakpoint queue), exercised with a probe device independent of
//! the transmission line that first uses it.
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use ngspice_rs::analysis::{
    AnalysisContext, AnalysisRequest, Plot, TransientStats, companion_transient,
};
use ngspice_rs::devices::delay::{DelayContext, DelayHistory, DelayLine, DelayUpdate};
use ngspice_rs::devices::{
    AcceptContext, AnalysisMode, Circuit, DelayAcceptance, Device, LinearContext, LoadRequest,
    ModelContext, StampContext, StateHistory,
};
use ngspice_rs::maths::{SparseMatrix, Vector};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, NodeId, SpiceError, SpiceResult};

#[derive(Debug, Clone, PartialEq)]
enum Event {
    /// A transient load at `time` saw `len` samples, the newest at `last`.
    Load { time: f64, len: usize, last: f64 },
    /// A delay accept at `time` saw `len` samples.
    Accept { time: f64, len: usize },
}

type Log = Rc<RefCell<Vec<Event>>>;

/// A 1 S conductance from `node` to ground that records `v(node)` at every
/// accepted point, requests one breakpoint at `request` and rejects steps
/// longer than 2 us that would end inside `squeeze` with half their size.
#[derive(Debug)]
struct Probe {
    terminals: [NodeId; 2],
    request: Option<f64>,
    squeeze: Option<(f64, f64)>,
    log: Log,
}

impl Device for Probe {
    fn name(&self) -> &str {
        "probe"
    }
    fn designator(&self) -> char {
        'x'
    }
    fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }
    fn delay_line(&self) -> Option<&dyn DelayLine> {
        Some(self)
    }
    fn stamp(&self, context: &mut StampContext<'_>) -> SpiceResult<()> {
        context.stamp(self.terminals[0], self.terminals[0], 1.)?;
        if let AnalysisMode::Transient { time, .. } = context.mode {
            let history = context
                .states
                .delay_history()
                .ok_or_else(|| SpiceError::circuit("no delay history in a transient load"))?;
            self.log.borrow_mut().push(Event::Load {
                time,
                len: history.len(),
                last: history.last_time().unwrap_or(f64::NAN),
            });
        }
        Ok(())
    }
    fn assemble_linear(&self, context: &mut LinearContext<'_>) -> SpiceResult<()> {
        context.nodal(self.terminals, 1., false)
    }
    fn accept(&self, context: &AcceptContext<'_>) -> SpiceResult<()> {
        if context.time == Some(99.) {
            return Err(SpiceError::circuit("accept hook refused"));
        }
        Ok(())
    }
}

impl DelayLine for Probe {
    fn width(&self) -> usize {
        1
    }
    fn start(&self, context: &DelayContext<'_>) -> SpiceResult<DelayUpdate> {
        Ok(DelayUpdate {
            reset: Some(vec![(
                context.time,
                vec![context.node_voltage(self.terminals[0])],
            )]),
            ..DelayUpdate::default()
        })
    }
    fn accept(
        &self,
        history: &DelayHistory,
        context: &DelayContext<'_>,
    ) -> SpiceResult<DelayUpdate> {
        if context.time == 98. {
            return Err(SpiceError::circuit("delay accept refused"));
        }
        self.log.borrow_mut().push(Event::Accept {
            time: context.time,
            len: history.len(),
        });
        Ok(DelayUpdate {
            append: Some((context.time, vec![context.node_voltage(self.terminals[0])])),
            breakpoints: self
                .request
                .filter(|t| *t > context.time)
                .into_iter()
                .collect(),
            ..DelayUpdate::default()
        })
    }
    fn timestep_limit(
        &self,
        _history: &DelayHistory,
        context: &DelayContext<'_>,
    ) -> SpiceResult<Option<f64>> {
        Ok(self
            .squeeze
            .filter(|(from, to)| (*from..*to).contains(&context.time) && context.steps[0] > 2e-6)
            .map(|_| 0.5 * context.steps[0]))
    }
}

fn with_probe(body: &str, request: Option<f64>, squeeze: Option<(f64, f64)>) -> (Circuit, Log) {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("delay.cir"),
            &format!("delay probe\n{body}\n.end\n"),
        ))
        .unwrap();
    let mut circuit = Circuit::from_netlist(&netlist).unwrap();
    let node = circuit.nodes().get("b").unwrap();
    let log = Log::default();
    circuit
        .add_device(Box::new(Probe {
            terminals: [node, NodeId::GROUND],
            request,
            squeeze,
            log: Rc::clone(&log),
        }))
        .unwrap();
    circuit.finalize().unwrap();
    (circuit, log)
}

const RC: &str = "v1 a 0 pwl(0 0 1m 1)\nr1 a b 1k\nc1 b 0 1u";

fn run(circuit: &mut Circuit, args: &[&str]) -> SpiceResult<(Plot, TransientStats)> {
    companion_transient(
        circuit,
        &AnalysisRequest::with_arguments(AnalysisKind::Transient, args.iter().copied()),
        &AnalysisContext::default(),
    )
}

fn times(plot: &Plot) -> Vec<f64> {
    (0..plot.point_count())
        .map(|i| plot.value("time", i).unwrap().re)
        .collect()
}

#[test]
fn device_breakpoints_are_landed_exactly() {
    let request = 0.371e-3;
    let (mut plain, _) = with_probe(RC, None, None);
    let (plot, stats) = run(&mut plain, &["10u", "1m"]).unwrap();
    assert!(!times(&plot).contains(&request));
    let (mut circuit, _) = with_probe(RC, Some(request), None);
    let (with, with_stats) = run(&mut circuit, &["10u", "1m"]).unwrap();
    let landed = times(&with);
    assert!(landed.contains(&request), "{landed:?}");
    // The extra breakpoint adds a landing (and the short restart step after
    // it); nothing else about the run changes before it.
    assert_eq!(with_stats.breakpoints, stats.breakpoints + 1);
    let before: Vec<_> = times(&plot).into_iter().filter(|t| *t < 0.3e-3).collect();
    assert_eq!(&landed[..before.len()], &before[..]);
    assert!(
        (plot.value("v(b)", plot.point_count() - 1).unwrap().re
            - with.value("v(b)", with.point_count() - 1).unwrap().re)
            .abs()
            < 1e-4
    );
}

#[test]
fn the_history_holds_only_accepted_points() {
    // Rejections forced between 0.2 ms and 0.25 ms.
    let (mut circuit, log) = with_probe(RC, None, Some((0.2e-3, 0.25e-3)));
    let (plot, stats) = run(&mut circuit, &["10u", "1m"]).unwrap();
    assert!(stats.rejected > 0, "{stats:?}");
    let accepted = times(&plot);
    let log = log.borrow();
    let mut accepts = 0;
    let mut rejected_loads = 0;
    for event in log.iter() {
        match *event {
            Event::Accept { time, len } => {
                accepts += 1;
                // The start sample plus one per earlier accepted point.
                assert_eq!(len, accepts, "at {time:e}");
                assert_eq!(accepted[accepts], time);
            }
            Event::Load { time, len, last } => {
                // A trial sees exactly the accepted history before it.
                assert_eq!(len, accepts + 1, "at {time:e}");
                assert_eq!(last, accepted[accepts]);
                assert!(time > last);
                if !accepted.contains(&time) {
                    rejected_loads += 1;
                }
            }
        }
    }
    assert_eq!(accepts, stats.accepted);
    // Rejected trials loaded (and read the history) at times that were
    // never accepted, without leaving a sample behind.
    assert!(rejected_loads > 0);
}

#[test]
fn failed_acceptance_leaves_every_history_unchanged() {
    let (circuit, _) = with_probe(RC, None, None);
    let n = circuit.unknown_count();
    let mut history = circuit.state_history();
    assert!(history.delay(3).is_some_and(DelayHistory::is_empty));
    let context = ModelContext::default();
    let x = Vector::from_slice(&vec![0.5; n]);
    let trial = |history: &StateHistory| {
        let mut trial = history.trial();
        circuit
            .load(
                &LoadRequest {
                    mode: AnalysisMode::OperatingPoint,
                    solution: &x,
                    model_context: &context,
                    integration: None,
                    history,
                    forcing: None,
                },
                &mut SparseMatrix::new(n, n),
                &mut Vector::zeros(n),
                &mut trial,
            )
            .unwrap();
        trial
    };
    let acceptance = |start| DelayAcceptance {
        steps: [1e-6; 3],
        min_break: 1e-12,
        start,
    };
    // A delay accept before the start is refused.
    let first = trial(&history);
    assert!(
        circuit
            .accept_transient_point(&x, 1e-6, &mut history.clone(), first, &acceptance(false))
            .is_err()
    );
    let first = trial(&history);
    let requested = circuit
        .accept_transient_point(&x, 0., &mut history, first, &acceptance(true))
        .unwrap();
    assert!(requested.is_empty());
    assert_eq!(history.delay(3).map(DelayHistory::len), Some(1));
    for time in [98., 99.] {
        let before = history.clone();
        let refused = trial(&history);
        assert!(
            circuit
                .accept_transient_point(&x, time, &mut history, refused, &acceptance(false))
                .is_err()
        );
        assert_eq!(history, before, "t = {time}");
    }
    let next = trial(&history);
    circuit
        .accept_transient_point(&x, 1e-6, &mut history, next, &acceptance(false))
        .unwrap();
    assert_eq!(history.delay(3).map(DelayHistory::len), Some(2));
    assert_eq!(history.delay(3).and_then(|h| h.value(1, 0)), Some(0.5));
}

#[test]
fn backends_without_a_delay_formulation_refuse_delay_lines() {
    let (mut circuit, _) = with_probe(RC, None, None);
    let error = run(
        &mut circuit,
        &["10u", "1m", "backend=diffsol", "method=bdf"],
    );
    // The companion entry point refuses diffsol options; the dispatcher
    // refuses the backend.
    assert!(error.is_err());
    let error = ngspice_rs::analysis::runner(AnalysisKind::Transient)
        .and_then(|driver| {
            driver.run_plots(
                &mut circuit,
                &AnalysisRequest::with_arguments(
                    AnalysisKind::Transient,
                    ["10u", "1m", "backend=diffsol", "method=bdf"],
                ),
                &AnalysisContext::default(),
            )
        })
        .unwrap_err();
    assert!(error.to_string().contains("delayed wave"), "{error}");
    let mut uic = AnalysisRequest::with_arguments(AnalysisKind::Transient, ["10u", "1m"]);
    uic.uic = true;
    let error = companion_transient(&mut circuit, &uic, &AnalysisContext::default()).unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
}
