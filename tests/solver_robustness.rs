//! Degenerate circuits through every analysis: each either fails as C
//! ngspice does (same class of error, naming the same unknown) or gives C's
//! answer. Expected values come from the live C binary with `.options sparse`
//! (`set numdgt=17`, `print all`); C's error classes from its batch output.
//!
//! Where C recovers from a failed operating point with its transient
//! fallback (`OPtran`, `optran.c`), which the port has not ported, the port
//! fails explicitly instead; those cases only check the singular unknown.

use ngspice_rs::analysis::{Plot, RunConfig, runner};
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::source::parse_deck_text;
use ngspice_rs::primitives::SpiceError;
use std::path::Path;

fn simulate(body: &str) -> Result<Plot, SpiceError> {
    let n = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("robust.cir"),
            &format!("t\n{body}\n.end\n"),
        ))
        .unwrap_or_else(|error| panic!("{error}\n{body}"));
    let c = RunConfig::from_netlist(&n)?;
    let request = c.request_for(&n.analyses[0])?;
    let mut circuit = c.circuit(&n)?;
    runner(request.kind)?.run(&mut circuit, &request, &c.context())
}

fn value(plot: &Plot, name: &str) -> f64 {
    plot.value(name, plot.point_count() - 1)
        .unwrap_or_else(|| panic!("no {name}"))
        .re
}

/// The deck fails with a numerical error naming `node` as `NIiter`'s
/// warning does (`singular matrix:  check node <node>`).
fn singular_at(body: &str, node: &str) {
    let error = simulate(body).expect_err(body);
    assert!(
        matches!(error, SpiceError::Numerical { .. }),
        "{body}: {error}"
    );
    let text = error.to_string();
    assert!(
        text.contains(&format!("singular matrix:  check node {node}")),
        "{body}: {text}"
    );
}

const VLOOP: &str = "v1 a 0 1\nv2 a 0 2\nr1 a 0 1k";

/// Parallel voltage sources: C fails every analysis (`DC solution failed`,
/// `AC operating point failed`, `Transient op failed`) after warning
/// `singular matrix:  check node v1#branch`.
#[test]
fn voltage_source_loops_fail_in_every_analysis() {
    for analysis in [".op", ".dc v1 0 1 0.5", ".tran 1u 10u"] {
        singular_at(&format!("{VLOOP}\n{analysis}"), "v1#branch");
    }
    singular_at(
        "v1 a 0 1 ac 1\nv2 a 0 2\nr1 a 0 1k\n.ac dec 2 1 100",
        "v1#branch",
    );
    // Equal values are still a loop.
    singular_at("v1 a 0 1\nv2 a 0 1\nr1 a 0 1k\n.op", "v1#branch");
    // An inductor is a DC short: in parallel with a source it is a loop.
    singular_at("v1 a 0 1\nl1 a 0 1m\n.op", "l1#branch");
    // A VCVS sensing its own output with unit gain has no equation.
    singular_at(
        "v1 a 0 1\nr1 a 0 1k\ne1 b 0 b 0 1\nr2 b 0 1k\n.op",
        "e1#branch",
    );
    singular_at("v1 a 0 1\ne1 a 0 a 0 2\nr1 a 0 1k\n.op", "v1#branch");
}

/// Nodes without a DC path: singular at the operating point in C (which
/// then recovers through `OPtran`; the port reports the failure).
#[test]
fn floating_nodes_fail_at_the_operating_point() {
    singular_at("v1 a 0 1\nr1 a 0 1k\nr2 x y 1k\n.op", "x");
    singular_at("v1 a 0 1\nr1 a b 1k\nc1 b c 1u\nc2 c 0 1u\n.op", "c");
    singular_at("i1 0 a 1m\nc1 a 0 1u\n.tran 1u 10u", "a");
    // C: `doAnalyses: matrix is singular` after the operating point.
    singular_at("v1 a 0 0 ac 1\nr1 a 0 1k\nr2 x y 1k\n.ac dec 2 10 1k", "x");
}

/// With `uic` there is no operating point, and the capacitor companion
/// makes the transient matrix regular: C and the port both simulate.
#[test]
fn floating_capacitor_nodes_simulate_with_uic() {
    let plot = simulate("i1 0 a 1m\nc1 a 0 1u\n.tran 1u 10u uic").unwrap();
    // 1 mA into 1 uF for 10 us.
    let v = value(&plot, "v(a)");
    assert!((v - 1e-2).abs() < 1e-9, "{v}");
}

/// A node joined only to current sources has no matrix element, so SPARSE
/// never numbers it and C solves the rest, leaving the node's right-hand
/// side (the net injected current, here 2 mA - 1 mA) as its "voltage":
/// C prints `a = 1.00000000000000002e-03`, `b = 1.0`. The port matches it
/// (with a warning) in every analysis.
#[test]
fn current_source_cutset_keeps_the_right_hand_side_as_c_does() {
    let body = "i1 0 a 2m\ni2 a b 1m\nr1 b 0 1k";
    let plot = simulate(&format!("{body}\n.op")).unwrap();
    assert_eq!(value(&plot, "v(a)"), 1e-3);
    assert_eq!(value(&plot, "v(b)"), 1.0);
    let plot = simulate(&format!("{body}\n.tran 1u 10u")).unwrap();
    assert_eq!(value(&plot, "v(b)"), 1.0);
    simulate("i1 0 a 0 ac 2m\ni2 a b 0 ac 1m\nr1 b 0 1k\n.ac lin 1 1k 1k").unwrap();
}

/// A conductance below `pivtol` (1e-14 < 1e-13) is a small pivot: SPARSE
/// takes the largest element anyway (`spSMALL_PIVOT`, which ngspice treats
/// as `OK`), so C solves it exactly: `a = 1.0e+05`. The port warns.
#[test]
fn small_pivots_are_accepted_like_c() {
    for options in ["", ".options pivtol=1e-16"] {
        let plot = simulate(&format!("i1 0 a 1n\nr1 a 0 1e14\n{options}\n.op")).unwrap();
        assert_eq!(value(&plot, "v(a)"), 1e5, "{options}");
    }
}

/// `resparam.c` raises a zero resistance to `RESMIN = 1e-12` (with a
/// warning). The 1e12 S conductance then loses digits, and C's answer
/// (`b = 9.99999999999999001e-01`, `v1#branch = -9.7656250000000000e-04`)
/// is reproduced exactly.
#[test]
fn zero_resistance_is_clamped_like_c() {
    for r1 in ["r1 a b 0", "r1 a b rm r=0\n.model rm r"] {
        let plot = simulate(&format!("v1 a 0 1\n{r1}\nr2 b 0 1k\n.op")).unwrap();
        assert_eq!(value(&plot, "v(b)"), 9.999_999_999_999_99e-1, "{r1}");
        assert_eq!(value(&plot, "i(v1)"), -9.765_625e-4, "{r1}");
    }
}

/// Zero capacitance and inductance are legal in C: a capacitor that stamps
/// nothing and an inductor that is a short at every analysis.
#[test]
fn zero_capacitance_and_inductance_simulate() {
    let plot = simulate("v1 a 0 1\nr1 a b 1k\nl1 b c 0\nr2 c 0 1k\n.op").unwrap();
    assert_eq!(value(&plot, "v(c)"), 0.5);
    assert_eq!(value(&plot, "i(l1)"), 5e-4);
    let plot = simulate("v1 a 0 1\nr1 a b 1k\nc1 b 0 0\nr2 b 0 1k\n.op").unwrap();
    assert_eq!(value(&plot, "v(b)"), 0.5);
    for body in [
        "v1 a 0 pulse(0 1 1u 1u 1u 5u 20u)\nr1 a b 1k\nc1 b 0 0\n.tran 1u 20u",
        "v1 a 0 pulse(0 1 1u 1u 1u 5u 20u)\nr1 a b 1k\nl1 b 0 0\nr2 b 0 1k\n.tran 1u 20u",
        // M = k sqrt(L1 L2) = 0 for a zero inductance.
        "v1 a 0 pulse(0 1 1u 1u 1u 5u 20u)\nr1 a b 1k\nl1 b 0 0\nl2 c 0 1m\nr2 c 0 1k\n\
         k1 l1 l2 0.5\n.tran 1u 20u",
        "v1 a 0 0 ac 1\nr1 a b 1k\nc1 b 0 0\n.ac dec 2 10 1k",
    ] {
        simulate(body).unwrap_or_else(|error| panic!("{body}: {error}"));
    }
}
