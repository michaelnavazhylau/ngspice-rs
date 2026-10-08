//! Linear controlled sources E/F/G/H (GitHub #78): stamps and sign
//! conventions, controlling-branch resolution (including hierarchical names
//! inside subcircuits) and explicit errors.
//!
//! Stamps are compared with the C load routines (`vcvsload.c`, `vccsload.c`,
//! `cccsload.c`, `ccvsload.c`), with ground rows and columns eliminated.
use std::path::Path;

use spice_core::{NodeId, Real, SpiceError};
use spice_devices::{
    AnalysisMode, Circuit, ControlReference, ControlledKind, ControlledSource, Device, LoadRequest,
    ModelContext, Registry,
};
use spice_maths::{SparseMatrix, Vector};
use spice_netlist::{Parser, RawCard, ast::Netlist, source::parse_deck_text};

fn deck(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("controlled.cir"),
            &format!("controlled sources\n{body}\n.end\n"),
        ))
        .unwrap()
}

fn circuit(body: &str) -> spice_core::SpiceResult<Circuit> {
    Circuit::from_netlist(&deck(body))
}

fn row(circuit: &Circuit, node: &str) -> usize {
    circuit
        .unknowns()
        .node_row(circuit.nodes().get(node).expect("node exists"))
        .expect("not ground")
}

fn index(circuit: &Circuit, name: &str) -> usize {
    circuit
        .devices()
        .iter()
        .position(|device| device.name() == name)
        .expect("device exists")
}

fn branch(circuit: &Circuit, name: &str) -> usize {
    let rows = circuit.branch_rows(index(circuit, name)).unwrap();
    assert_eq!(rows.len(), 1, "{name}");
    rows.start
}

/// The operating point of a linear deck: one LU solve of the assembly.
fn solve(circuit: &mut Circuit) -> Vector {
    let system = circuit.linear_system().unwrap();
    system.a.solve(&system.dc_rhs(None).unwrap()).unwrap()
}

fn close(got: Real, want: Real) {
    assert!(
        (got - want).abs() <= 1e-12 * want.abs() + 1e-15,
        "{got} != {want}"
    );
}

#[test]
fn vcvs_stamps_match_vcvsload_and_enforce_the_gain() {
    let mut c = circuit("vin in 0 dc 2\nrin in 0 1k\ne1 out 0 in 0 3\nrl out 0 1k").unwrap();
    let a = c.linear_system().unwrap().a;
    let (out, input, k) = (row(&c, "out"), row(&c, "in"), branch(&c, "e1"));
    assert_eq!(a.get(out, k), 1.0);
    assert_eq!(a.get(k, out), 1.0);
    assert_eq!(a.get(k, input), -3.0);
    let x = solve(&mut c);
    close(x.as_slice()[out], 6.0);
    // The branch current is positive from n+ through the source to n-: the
    // source delivers 6 mA into the 1k load, so its current is -6 mA.
    close(x.as_slice()[k], -6e-3);
    // The controlling port draws no current: vin only feeds rin.
    close(x.as_slice()[branch(&c, "vin")], -2e-3);
}

#[test]
fn vccs_draws_current_out_of_its_positive_terminal() {
    let mut c = circuit("vin in 0 dc 2\nrin in 0 1k\ng1 a 0 in 0 1m\nra a 0 1k").unwrap();
    let a = c.linear_system().unwrap().a;
    let (node, input) = (row(&c, "a"), row(&c, "in"));
    // Only conductance (a,a) from ra and the transconductance (a,in).
    assert_eq!(a.get(node, input), 1e-3);
    assert_eq!(c.branch_rows(index(&c, "g1")).unwrap().len(), 0);
    let x = solve(&mut c);
    // 2 mA flow from a through g1 to ground, pulled through ra: v(a) = -2 V.
    close(x.as_slice()[node], -2.0);
    // Swapping the controlling nodes flips the sign.
    let mut c = circuit("vin in 0 dc 2\nrin in 0 1k\ng1 a 0 0 in 1m\nra a 0 1k").unwrap();
    let node = row(&c, "a");
    close(solve(&mut c).as_slice()[node], 2.0);
}

#[test]
fn cccs_senses_the_controlling_branch_current() {
    let mut c = circuit("vin in 0 dc 2\nrin in 0 1k\nf1 a 0 vin 2\nra a 0 1k").unwrap();
    let a = c.linear_system().unwrap().a;
    let (node, sensed) = (row(&c, "a"), branch(&c, "vin"));
    assert_eq!(a.get(node, sensed), 2.0);
    assert_eq!(c.control_rows(index(&c, "f1")).unwrap(), vec![sensed]);
    let x = solve(&mut c);
    // i(vin) = -2 mA; f1 carries 2 i(vin) = -4 mA from a to ground, i.e. it
    // pushes 4 mA into a: v(a) = +4 V.
    close(x.as_slice()[sensed], -2e-3);
    close(x.as_slice()[node], 4.0);
}

#[test]
fn ccvs_adds_a_branch_and_a_transresistance() {
    let mut c = circuit("vin in 0 dc 2\nrin in 0 1k\nh1 a 0 vin 100\nra a 0 1k").unwrap();
    let a = c.linear_system().unwrap().a;
    let (node, sensed, k) = (row(&c, "a"), branch(&c, "vin"), branch(&c, "h1"));
    assert_eq!(a.get(k, sensed), -100.0);
    assert_eq!(a.get(node, k), 1.0);
    assert_eq!(a.get(k, node), 1.0);
    let x = solve(&mut c);
    close(x.as_slice()[node], -0.2);
    // 0.2 mA flows from ground through ra into a, then through h1 to ground.
    close(x.as_slice()[k], 2e-4);
}

#[test]
fn every_analysis_mode_loads_the_same_static_stamp() {
    let mut c = circuit(
        "vin in 0 dc 2\nrin in 0 1k\ne1 o1 0 in 0 3\nr1 o1 0 1k\ng1 o2 0 in 0 1m\nr2 o2 0 1k\n\
         f1 o3 0 vin 2\nr3 o3 0 1k\nh1 o4 0 vin 100\nr4 o4 0 1k",
    )
    .unwrap();
    let linear = c.linear_system().unwrap().a;
    let n = c.unknown_count();
    let history = c.state_history();
    let solution = Vector::zeros(n);
    let context = ModelContext::default();
    for mode in [
        AnalysisMode::OperatingPoint,
        AnalysisMode::DcSweep,
        AnalysisMode::Ac { frequency: 1e3 },
    ] {
        let mut matrix = SparseMatrix::new(n, n);
        let mut rhs = Vector::zeros(n);
        let mut trial = history.trial();
        c.load(
            &LoadRequest {
                mode,
                solution: &solution,
                model_context: &context,
                integration: None,
                history: &history,
                forcing: None,
            },
            &mut matrix,
            &mut rhs,
            &mut trial,
        )
        .unwrap_or_else(|error| {
            // Independent sources refuse AC loads; the controlled sources
            // themselves never fail in any mode.
            assert!(matches!(mode, AnalysisMode::Ac { .. }), "{error}");
        });
        if matches!(mode, AnalysisMode::Ac { .. }) {
            continue;
        }
        for r in 0..n {
            for col in 0..n {
                assert_eq!(
                    matrix.get(r, col),
                    linear.get(r, col),
                    "({r},{col}) {mode:?}"
                );
            }
        }
    }
}

#[test]
fn controlling_sources_may_follow_their_users_and_be_e_or_h_branches() {
    // f1 names e1, which is declared later; h2 senses h1's branch.
    let mut c = circuit(
        "vin in 0 dc 1\nrin in 0 1k\nf1 0 a e1 0.5\nra a 0 1k\ne1 b 0 in 0 4\nrb b 0 2k\n\
         h1 c 0 vin 1k\nrc c 0 1k\nh2 d 0 h1 1k\nrd d 0 1k",
    )
    .unwrap();
    let x = solve(&mut c);
    let x = x.as_slice();
    // i(e1) = -v(b)/2k = -2 mA; f1 0 a carries 0.5 i(e1) = -1 mA from 0
    // through itself to a, i.e. it draws 1 mA out of a: v(a) = -1 V.
    close(x[branch(&c, "e1")], -2e-3);
    close(x[row(&c, "a")], -1.0);
    // i(vin) = -1 mA: v(c) = -1 V, i(h1) = +1 mA (from ground through rc).
    close(x[row(&c, "c")], -1.0);
    close(x[branch(&c, "h1")], 1e-3);
    close(x[row(&c, "d")], 1.0);
}

#[test]
fn subcircuit_controls_resolve_to_hierarchical_branches() {
    let body = ".subckt mirror in out\nvs in mid 0\nrs mid 0 1k\nf1 0 out vs 2\n.ends mirror\n\
                vin a 0 dc 1\nx1 a o1 mirror\nr1 o1 0 1k\nvb b 0 dc 3\nx2 b o2 mirror\nr2 o2 0 1k";
    let mut c = circuit(body).unwrap();
    let names: Vec<_> = c.devices().iter().map(|d| d.name().to_owned()).collect();
    assert!(names.contains(&"f.x1.f1".to_owned()), "{names:?}");
    let f1 = c.device("f.x1.f1").unwrap();
    assert_eq!(f1.controlling_sources()[0].name, "v.x1.vs");
    let f2 = c.device("f.x2.f1").unwrap();
    assert_eq!(f2.controlling_sources()[0].name, "v.x2.vs");
    assert_eq!(
        c.control_rows(index(&c, "f.x2.f1")).unwrap(),
        vec![branch(&c, "v.x2.vs")]
    );
    let x = solve(&mut c);
    // Each mirror copies twice its own input current (1 mA and 3 mA).
    close(x.as_slice()[row(&c, "o1")], 2.0);
    close(x.as_slice()[row(&c, "o2")], 6.0);
    // A subcircuit cannot sense a top-level source by its bare name: C renames
    // the reference into the instance (`translate_inst_name`), which then
    // names nothing.
    let error = circuit(
        ".subckt s out\nf1 0 out vtop 1\nr1 out 0 1k\n.ends s\nvtop a 0 dc 1\nra a 0 1k\nx1 o s",
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("f.x1.f1: unknown controlling source v.x1.vtop"),
        "{error}"
    );
}

#[test]
fn missing_and_unfindable_controlling_sources_are_errors() {
    let error = circuit("vin in 0 dc 1\nrin in 0 1k\nf1 a 0 vnone 2\nra a 0 1k").unwrap_err();
    assert!(matches!(error, SpiceError::Parse { .. }), "{error:?}");
    assert!(
        error
            .to_string()
            .contains("controlled.cir:4:8: f1: unknown controlling source vnone"),
        "{error}"
    );
    // C's CKTfndBranch finds V/E/H branches only: an inductor or a resistor
    // (or a current source, a G or an F) cannot control an F/H.
    for (body, target) in [
        (
            "vin in 0 dc 1\nl1 in x 1m\nrx x 0 1k\nh1 a 0 l1 2\nra a 0 1k",
            "l1",
        ),
        ("vin in 0 dc 1\nrin in 0 1k\nh1 a 0 rin 2\nra a 0 1k", "rin"),
        ("i1 0 in dc 1m\nrin in 0 1k\nf1 a 0 i1 2\nra a 0 1k", "i1"),
        (
            "vin in 0 dc 1\nrin in 0 1k\ng1 b 0 in 0 1m\nrb b 0 1k\nf1 a 0 g1 2\nra a 0 1k",
            "g1",
        ),
    ] {
        let error = circuit(body).unwrap_err();
        assert!(
            error.to_string().contains(&format!(
                "controlling source {target} has no findable branch"
            )),
            "{error}"
        );
    }
}

#[test]
fn shorted_voltage_outputs_are_rejected_like_c_setup() {
    for (body, kind) in [
        ("vin in 0 dc 1\nrin in 0 1k\ne1 a a in 0 2", "VCVS"),
        ("vin in 0 dc 1\nrin in 0 1k\nh1 0 0 vin 2", "CCVS"),
    ] {
        let error = circuit(body).unwrap_err();
        assert!(
            error.to_string().contains(&format!("is a shorted {kind}")),
            "{error}"
        );
    }
    // A G/F with both outputs on one node is merely inert, as in C.
    assert!(circuit("vin in 0 dc 1\nrin in 0 1k\ng1 a a in 0 2\nra a 0 1k").is_ok());
}

#[test]
fn setter_order_and_multiplier_follow_vccsparam() {
    let c = circuit(
        "vin in 0 dc 1\nrin in 0 1k\ng1 0 a in 0 1m m=2\nra a 0 1k\ng2 0 b in 0 gain=1m m=2\n\
         rb b 0 1k\nf1 0 c vin 1 m=2 gain=3\nrc c 0 1k\ne1 d 0 in 0 {2*3}\nrd d 0 1k",
    )
    .unwrap();
    let mut c = c;
    let a = c.linear_system().unwrap().a;
    let input = row(&c, "in");
    // Leading 1m is applied after m=2: 2 mS. gain=1m precedes m=2: 1 mS.
    assert_eq!(a.get(row(&c, "a"), input), -2e-3);
    assert_eq!(a.get(row(&c, "b"), input), -1e-3);
    // f1: gain=3 after m=2 gives 6, then the leading 1 after m=2 gives 2.
    assert_eq!(a.get(row(&c, "c"), branch(&c, "vin")), -2.0);
    // A literalized {2*3} gain.
    assert_eq!(a.get(branch(&c, "e1"), input), -6.0);
}

#[test]
fn programmatic_construction_validates_kinds_and_gains() {
    let (a, b) = (NodeId::GROUND, NodeId::GROUND);
    assert!(
        ControlledSource::voltage_controlled("e1", ControlledKind::Cccs, [a, b], [a, b], 1.0)
            .is_err()
    );
    let control = ControlReference {
        name: "v1".into(),
        location: None,
    };
    assert!(
        ControlledSource::current_controlled(
            "g1",
            ControlledKind::Vccs,
            [a, b],
            control.clone(),
            1.0
        )
        .is_err()
    );
    let mut nodes = spice_core::NodeTable::new();
    let (p, n) = (nodes.intern("p"), nodes.intern("n"));
    assert!(
        ControlledSource::current_controlled(
            "f1",
            ControlledKind::Cccs,
            [p, n],
            control,
            Real::NAN
        )
        .is_err()
    );
    let g = ControlledSource::voltage_controlled("g1", ControlledKind::Vccs, [p, n], [p, n], 2.0)
        .unwrap();
    assert_eq!(g.designator(), 'g');
    assert_eq!(g.terminals().len(), 4);
    assert_eq!(g.branch_currents(), 0);
    assert_eq!(g.findable_branch(), None);
    assert!(!g.is_nonlinear());
}

#[test]
fn the_registry_builds_all_four_from_cards() {
    let registry = Registry::with_builtins();
    for designator in ['e', 'f', 'g', 'h'] {
        assert!(registry.get(designator).unwrap().ported, "{designator}");
    }
    let card = |text: &str| {
        let deck = parse_deck_text(Path::new("card.cir"), &format!("title\n{text}\n"));
        RawCard::parse(&deck.lines[0]).unwrap()
    };
    let mut nodes = spice_core::NodeTable::new();
    for (text, terminals, branches) in [
        ("e1 a 0 b 0 2", 4, 1),
        ("g1 a 0 b 0 2m", 4, 0),
        ("f1 a 0 v1 2", 2, 0),
        ("h1 a 0 v1 2", 2, 1),
    ] {
        let device = registry.instantiate(&card(text), &mut nodes).unwrap();
        assert_eq!(device.terminals().len(), terminals, "{text}");
        assert_eq!(device.branch_currents(), branches, "{text}");
        assert_eq!(
            device.findable_branch(),
            (branches == 1).then_some(0),
            "{text}"
        );
    }
    let error = registry
        .instantiate(&card("e1 a 0 poly(1) b 0 0 1"), &mut nodes)
        .unwrap_err();
    // A single-card factory cannot build the front end's rewrite (#79).
    assert!(
        error.to_string().contains("lower_nonlinear_sources"),
        "{error}"
    );
}
