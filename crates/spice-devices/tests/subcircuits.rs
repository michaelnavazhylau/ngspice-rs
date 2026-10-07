//! Subcircuit instantiation: port binding, hierarchical identity, scoped
//! parameters/models and atomic rejection (GitHub #18).
//!
//! Numeric checks here use the immutable linear assembly, because a purely
//! resistive instance is solved by one LU factorization. The committed
//! `subckt_divider` golden is compared through the production `.op` path by
//! `cargo xtask golden verify` and by `spice-analysis`'s `golden_rawfiles` test.
use std::path::Path;
use std::sync::Arc;

use spice_core::{Real, SpiceError};
use spice_devices::subckt::{SubcircuitLimits, expand_subcircuits};
use spice_devices::{Circuit, ModelContext, ModelResolver};
use spice_netlist::eval::ParamScope;
use spice_netlist::{Parser, ast::Netlist, source::parse_deck_text};

fn deck(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("subckts.cir"),
            &format!("subcircuits\n{body}\n.end\n"),
        ))
        .unwrap()
}

fn circuit(body: &str) -> spice_core::SpiceResult<Circuit> {
    Circuit::from_netlist(&deck(body))
}

/// The operating point of a purely resistive deck: one linear solve.
fn node_voltage(circuit: &mut Circuit, name: &str) -> Real {
    let row = circuit
        .unknowns()
        .node_row(circuit.nodes().get(name).expect("node exists"))
        .expect("row");
    let system = circuit.linear_system().unwrap();
    let rhs = system.dc_rhs(None).unwrap();
    system.a.solve(&rhs).unwrap().as_slice()[row]
}

/// A resistor's supplied scalar, which for a literal resistor is its value.
fn supplied(circuit: &Circuit, name: &str) -> Real {
    circuit.resistor(name).expect("resistor exists").1.supplied
}

fn terminals(circuit: &Circuit, name: &str) -> Vec<String> {
    circuit
        .device(name)
        .expect("device exists")
        .terminals()
        .iter()
        .map(|id| circuit.nodes().node(*id).expect("node exists").name.clone())
        .collect()
}

#[test]
fn an_instance_binds_ports_and_simulates_like_the_committed_divider() {
    let mut circuit = circuit(
        ".subckt div a b\nr1 a b 1k\n.ends div\n\
         v1 in 0 dc 10\nx1 in out div\nr2 out 0 1k\n.op",
    )
    .unwrap();
    // The declaration inside the instance is renamed but keeps its terminal
    // binding; the load resistor stays a top-level device.
    assert_eq!(terminals(&circuit, "r.x1.r1"), ["in", "out"]);
    assert_eq!(supplied(&circuit, "r.x1.r1"), 1e3);
    assert_eq!(terminals(&circuit, "r2"), ["out", "0"]);
    assert_eq!(
        circuit
            .devices()
            .iter()
            .map(|device| device.name())
            .collect::<Vec<_>>(),
        ["v1", "r.x1.r1", "r2"],
        "deck order survives expansion"
    );
    // Neither the instance nor its definition appears as a device.
    assert_eq!(
        circuit
            .nodes()
            .nodes()
            .iter()
            .map(|n| n.name.as_str())
            .collect::<Vec<_>>(),
        ["0", "in", "out"]
    );
    assert_eq!(node_voltage(&mut circuit, "out"), 5.0);
}

#[test]
fn two_instances_of_one_definition_never_share_an_internal_identity() {
    let mut circuit = circuit(
        ".subckt div a b\nr1 a mid 1k\nr2 mid b 1k\n.ends div\n\
         v1 in 0 dc 10\nx1 in o1 div\nx2 in o2 div\nr3 o1 0 1k\nr4 o2 0 1k\n.op",
    )
    .unwrap();
    let first = circuit.nodes().get("x1.mid").expect("x1 internal node");
    let second = circuit.nodes().get("x2.mid").expect("x2 internal node");
    assert_ne!(first, second);
    assert_eq!(terminals(&circuit, "r.x1.r1"), ["in", "x1.mid"]);
    assert_eq!(terminals(&circuit, "r.x2.r2"), ["x2.mid", "o2"]);
    // Identical decks, identical results, distinct unknowns.
    assert_eq!(
        node_voltage(&mut circuit, "o1"),
        node_voltage(&mut circuit, "o2")
    );
}

#[test]
fn nested_instances_extend_the_hierarchical_path() {
    let mut circuit = circuit(
        ".subckt inner a b\nr1 a c 1k\nr2 c b 1k\n.ends inner\n\
         .subckt outer p q\nx1 p mid inner\nr3 mid q 1k\n.ends outer\n\
         v1 in 0 dc 10\nxout in out outer\nr4 out 0 1k\n.op",
    )
    .unwrap();
    // C: subckt.c::translate_node_name/translate_inst_name.
    assert!(circuit.nodes().get("xout.mid").is_some());
    assert!(circuit.nodes().get("xout.x1.c").is_some());
    assert_eq!(terminals(&circuit, "r.xout.r3"), ["xout.mid", "out"]);
    assert_eq!(terminals(&circuit, "r.xout.x1.r1"), ["in", "xout.x1.c"]);
    // 10 V across 2k (inside inner) + 1k + the 1k load: the tap sits at 5 V.
    assert!((node_voltage(&mut circuit, "xout.mid") - 5.0).abs() < 1e-12);
}

#[test]
fn instance_overrides_beat_formal_defaults_in_written_order() {
    let circuit = circuit(
        ".subckt div a b rval=2k\nr1 a b {rval}\n.ends div\n\
         v1 in 0 dc 10\n\
         x1 in o1 div\n\
         x2 in o2 div rval=3k\n\
         x3 in o3 div rval=1k rval=8k\n\
         x4 in o4 div unknown=7k\n",
    )
    .unwrap();
    assert_eq!(supplied(&circuit, "r.x1.r1"), 2e3, "formal default");
    assert_eq!(supplied(&circuit, "r.x2.r1"), 3e3, "instance override");
    assert_eq!(
        supplied(&circuit, "r.x3.r1"),
        8e3,
        "the last duplicate setter wins"
    );
    assert_eq!(
        supplied(&circuit, "r.x4.r1"),
        2e3,
        "an undeclared instance parameter is inert, as in C"
    );
}

#[test]
fn defaults_and_body_params_share_the_instance_scope() {
    // C: numparam evaluates the whole body in one environment, so a default may
    // reference a body `.param` (and the caller's scope through the parent), a
    // body `.param` may reference a formal, and a body `.param` outranks the
    // formal default it redefines.
    let circuit = circuit(
        ".param base=5k\n\
         .subckt bycaller a b rval={base*2}\nr1 a b {rval}\n.ends bycaller\n\
         .subckt bybody a b rval={inner}\n.param inner=4k\nr1 a b {rval}\n.ends bybody\n\
         .subckt viabody a b rval=2k\n.param doubled={rval*2}\nr1 a b {doubled}\n.ends viabody\n\
         .subckt bodywins a b rval=2k\n.param rval=9k\nr1 a b {rval}\n.ends bodywins\n\
         .subckt overridewins a b rval=2k\n.param rval=9k\nr1 a b {rval}\n.ends overridewins\n\
         v1 in 0 dc 10\n\
         x1 in o1 bycaller\nx2 in o2 bybody\nx3 in o3 viabody\n\
         x4 in o4 bodywins\nx5 in o5 overridewins rval=3k\n",
    )
    .unwrap();
    assert_eq!(supplied(&circuit, "r.x1.r1"), 10e3);
    assert_eq!(supplied(&circuit, "r.x2.r1"), 4e3);
    assert_eq!(supplied(&circuit, "r.x3.r1"), 4e3);
    assert_eq!(
        supplied(&circuit, "r.x4.r1"),
        9e3,
        "body .param beats default"
    );
    assert_eq!(
        supplied(&circuit, "r.x5.r1"),
        3e3,
        "instance override beats a body .param"
    );
}

#[test]
fn globals_and_ground_are_not_prefixed() {
    let mut circuit = circuit(
        ".global mid\n\
         .subckt div a b\nr1 a mid 1k\nr2 mid b 1k\n.ends div\n\
         v1 in 0 dc 10\nx1 in out div\nr3 out 0 1k\n.op",
    )
    .unwrap();
    assert!(circuit.nodes().get("x1.mid").is_none());
    assert_eq!(terminals(&circuit, "r.x1.r1"), ["in", "mid"]);
    assert_eq!(terminals(&circuit, "r.x1.r2"), ["mid", "out"]);
    // Ground inside a body is ground, not `x1.0`.
    assert!(circuit.nodes().get("x1.0").is_none());
    assert_eq!(terminals(&circuit, "r3"), ["out", "0"]);
    // mid is 10 V * 1k/(1k + 1k||1k) = 6.667 V.
    assert!((node_voltage(&mut circuit, "mid") - 20.0 / 3.0).abs() < 1e-12);
}

#[test]
fn a_body_grounded_device_uses_the_circuit_ground() {
    let mut circuit = circuit(
        ".subckt shunt a\nr1 a 0 2k\n.ends shunt\nv1 in 0 dc 10\nx1 in shunt\nr2 in 0 1k\n.op",
    )
    .unwrap();
    assert_eq!(terminals(&circuit, "r.x1.r1"), ["in", "0"]);
    // Serial 2k with the 1k load: v(a) = 10/3.
    assert!((node_voltage(&mut circuit, "in") - 10.0).abs() < 1e-12);
}

#[test]
fn a_local_model_shadows_the_root_declaration_inside_its_body_only() {
    let circuit = circuit(
        ".model dm r(r=2k)\n\
         .subckt div a b\n.model dm r(r=1k)\nr1 a b dm\n.ends div\n\
         r0 top 0 dm\nv1 in 0 dc 10\nx1 in out div\n",
    )
    .unwrap();
    assert_eq!(
        supplied(&circuit, "r.x1.r1"),
        1e3,
        "the body's own .model wins inside the instance"
    );
    assert_eq!(
        supplied(&circuit, "r0"),
        2e3,
        "the root declaration is untouched outside it"
    );
    assert_eq!(
        circuit
            .nodes()
            .nodes()
            .iter()
            .filter(|node| node.name == "top")
            .count(),
        1
    );
}

#[test]
fn a_source_inside_an_instance_keeps_its_branch_unknown() {
    let mut circuit =
        circuit(".subckt drive a b\nv1 a b dc 5\n.ends drive\nr1 load 0 1k\nx1 load 0 drive\n.op")
            .unwrap();
    let source = circuit.device("v.x1.v1").expect("renamed source");
    assert_eq!(source.branch_currents(), 1);
    assert_eq!(terminals(&circuit, "v.x1.v1"), ["load", "0"]);
    // v(load) = 5 V, so the resistor draws 5 mA from the source's positive
    // terminal: the branch unknown is negative, as for a top-level source.
    assert!((node_voltage(&mut circuit, "load") - 5.0).abs() < 1e-12);
    let row = circuit
        .unknowns()
        .node_row(circuit.nodes().get("load").unwrap())
        .unwrap();
    let system = circuit.linear_system().unwrap();
    let rhs = system.dc_rhs(None).unwrap();
    let solution = system.a.solve(&rhs).unwrap();
    let index = circuit
        .devices()
        .iter()
        .position(|device| device.name() == "v.x1.v1")
        .unwrap();
    let branch = circuit.branch_rows(index).unwrap();
    assert_eq!(branch.len(), 1);
    assert!((solution.as_slice()[row] - 5.0).abs() < 1e-12);
    assert!(solution.as_slice()[branch.start] < 0.0);
}

#[test]
fn missing_definitions_arity_mismatches_and_cycles_are_explicit() {
    let unknown = circuit("v1 in 0 dc 10\nx1 in out nosuch\n").unwrap_err();
    assert!(
        unknown.to_string().contains("unknown subcircuit 'nosuch'"),
        "{unknown}"
    );
    // The invocation card, not the missing definition, is what is located.
    assert!(
        unknown.to_string().starts_with("subckts.cir:3:"),
        "{unknown}"
    );
    let arity =
        circuit(".subckt div a b\nr1 a b 1k\n.ends div\nx1 in out extra div\n").unwrap_err();
    assert!(
        arity
            .to_string()
            .contains("has 2 terminal(s), but x1 supplies 3"),
        "{arity}"
    );
    let cycle = circuit(
        ".subckt first a b\nx1 a b second\n.ends first\n\
         .subckt second a b\nx1 a b first\n.ends second\n\
         v1 in 0 dc 10\nx9 in out first\n",
    )
    .unwrap_err();
    assert!(
        cycle.to_string().contains("circular subcircuit definition"),
        "{cycle}"
    );
    assert!(
        cycle.to_string().contains("'first' -> 'second' -> 'first'"),
        "{cycle}"
    );
    for error in [unknown, arity, cycle] {
        assert!(matches!(error, SpiceError::Parse { .. }), "{error:?}");
    }
}

#[test]
fn unsupported_bodies_are_rejected_rather_than_ignored() {
    let nested = circuit(
        ".subckt outer a b\n.subckt inner x y\nr1 x y 1k\n.ends inner\nr2 a b 1k\n.ends outer\n\
         v1 in 0 dc 10\nx1 in out outer\n",
    )
    .unwrap_err();
    assert!(nested.is_not_yet_ported(), "{nested}");
    assert!(nested.to_string().contains("nested subcircuit"), "{nested}");
    let analysis = circuit(
        ".subckt outer a b\nr2 a b 1k\n.op\n.ends outer\nv1 in 0 dc 10\nx1 in out outer\n.op\n",
    )
    .unwrap_err();
    assert!(analysis.is_not_yet_ported(), "{analysis}");
    assert!(
        analysis.to_string().contains("analysis card inside"),
        "{analysis}"
    );
}

#[test]
fn expansion_limits_are_enforced() {
    let netlist =
        deck(".subckt chain a b\nr1 a b 1k\n.ends chain\nv1 in 0 dc 10\nx1 in out chain\n");
    let scope = Arc::new(ParamScope::root(&netlist.params).unwrap());
    let limits = SubcircuitLimits {
        max_depth: 32,
        max_devices: 0,
    };
    let error = expand_subcircuits(&netlist, &scope, limits).unwrap_err();
    assert!(error.to_string().contains("more than 0 devices"), "{error}");
    let depth = SubcircuitLimits {
        max_depth: 0,
        max_devices: 250_000,
    };
    let error = expand_subcircuits(&netlist, &scope, depth).unwrap_err();
    assert!(
        error.to_string().contains("deeper than 0 levels"),
        "{error}"
    );
}

#[test]
fn expansion_is_pure_and_reports_the_definition_location() {
    let netlist =
        deck(".subckt div a b\nr1 a b {undefined}\n.ends div\nv1 in 0 dc 10\nx1 in out div\n");
    let scope = Arc::new(ParamScope::root(&netlist.params).unwrap());
    let before = netlist.clone();
    let error = expand_subcircuits(&netlist, &scope, SubcircuitLimits::default()).unwrap_err();
    assert!(error.to_string().contains("undefined parameter"), "{error}");
    // The failing site is the body card, not the invocation.
    assert!(error.to_string().contains("subckts.cir:3:"), "{error}");
    assert_eq!(netlist, before, "the input netlist is never modified");
}

#[test]
fn a_failed_batch_leaves_the_caller_circuit_untouched() {
    let netlist = deck("r1 a 0 1k\nr2 b 0 dm\n.model dm d(is=1e-14)\n");
    let mut circuit = Circuit::new();
    circuit.add_node("keep");
    let before: Vec<String> = circuit
        .nodes()
        .nodes()
        .iter()
        .map(|node| node.name.clone())
        .collect();
    let models = ModelResolver::new(&netlist.models).unwrap();
    let error = circuit
        .add_instances(&netlist.devices, &models, &ModelContext::default())
        .unwrap_err();
    assert!(error.to_string().contains("wrong model family"), "{error}");
    assert_eq!(circuit.device_count(), 0);
    assert_eq!(
        circuit
            .nodes()
            .nodes()
            .iter()
            .map(|node| node.name.clone())
            .collect::<Vec<_>>(),
        before,
        "no staged node is committed on failure"
    );
    // The same deck succeeds when the reference resolves.
    let good = deck("r1 a 0 1k\nr2 b 0 2k\n");
    circuit
        .add_instances(
            &good.devices,
            &ModelResolver::new(&good.models).unwrap(),
            &ModelContext::default(),
        )
        .unwrap();
    assert_eq!(circuit.device_count(), 2);
}

#[test]
fn a_duplicate_expanded_name_is_reported_without_committing() {
    let mut circuit = Circuit::new();
    let netlist = deck("r1 a 0 1k\nR1 b 0 1k\n");
    let models = ModelResolver::new(&netlist.models).unwrap();
    let error = circuit
        .add_instances(&netlist.devices, &models, &ModelContext::default())
        .unwrap_err();
    assert!(
        error.to_string().contains("duplicate instance name"),
        "{error}"
    );
    assert_eq!(circuit.device_count(), 0);
    assert_eq!(circuit.nodes().len(), 1);
}

#[test]
fn local_models_are_renamed_per_instance_and_do_not_leak_to_siblings() {
    // A body-local declaration belongs to that instance: two instances of one
    // definition get one renamed copy each, while a root declaration referenced
    // from a body keeps its own name.
    let netlist = deck(
        ".model rm r(r=2k)\n\
         .subckt div a b\n.model am r(r=1k)\nr1 a b am\n.ends div\n\
         .subckt usesroot a b\nr1 a b rm\n.ends usesroot\n\
         r9 t 0 rm\nv1 in 0 dc 10\nx1 in o1 div\nx2 in o2 div\nx3 in o3 usesroot\n",
    );
    let scope = Arc::new(ParamScope::root(&netlist.params).unwrap());
    let expanded = expand_subcircuits(&netlist, &scope, SubcircuitLimits::default()).unwrap();
    assert_eq!(
        expanded
            .models
            .iter()
            .map(|model| model.name.as_str())
            .collect::<Vec<_>>(),
        ["rm", "x1.am", "x2.am"],
        "root declaration unrenamed, one local copy per instance"
    );
    assert_eq!(
        expanded
            .devices
            .iter()
            .map(|device| device.model.as_deref().unwrap_or("-"))
            .collect::<Vec<_>>(),
        ["rm", "-", "x1.am", "x2.am", "rm"],
        "a body reaches the root declaration through the parent link"
    );
    // A sibling body cannot name it at all: the parser only inherits ancestor
    // declarations, so a sideways reference is rejected before elaboration.
    let leaky = Parser::new().parse_deck(&parse_deck_text(
        Path::new("subckts.cir"),
        "subcircuits\n.subckt a x y\n.model am r(r=1k)\nr1 x y am\n.ends a\n\
         .subckt b x y\nr1 x y am\n.ends b\n.end\n",
    ));
    assert!(leaky.is_err(), "a sibling must not see 'am'");
}

#[test]
fn gnd_is_ground_with_aliasing_and_only_global_under_no_auto_gnd() {
    // With automatic gnd aliasing the parser rewrites `gnd` to ground, so a body
    // node written `gnd` is ground and is never prefixed.
    let netlist = deck(".subckt s a\nr1 a gnd 1k\n.ends s\nv1 in 0 dc 1\nx1 in s\n");
    let scope = Arc::new(ParamScope::root(&netlist.params).unwrap());
    let expanded = expand_subcircuits(&netlist, &scope, SubcircuitLimits::default()).unwrap();
    assert_eq!(expanded.devices[1].nodes, ["in", "0"]);

    // Under `no_auto_gnd` the same word is an ordinary node: `.global gnd` keeps
    // it, and every other body node is prefixed as usual.
    let deck = parse_deck_text(
        Path::new("gnd.cir"),
        "no auto gnd\n.global gnd\n.subckt s a\nr1 a gnd 1k\nr2 a extra 1k\n.ends s\n\
         v1 in 0 dc 1\nx1 in s\n.end\n",
    );
    let netlist = Parser::with_auto_gnd(false).parse_deck(&deck).unwrap();
    let scope = Arc::new(ParamScope::root(&netlist.params).unwrap());
    let expanded = expand_subcircuits(&netlist, &scope, SubcircuitLimits::default()).unwrap();
    assert_eq!(expanded.devices[1].nodes, ["in", "gnd"]);
    assert_eq!(expanded.devices[2].nodes, ["in", "x1.extra"]);
}

#[test]
fn a_flat_deck_is_unchanged_by_expansion() {
    let netlist = deck("v1 in 0 dc 10\nr1 in out 1k\nr2 out 0 1k\n.op");
    let scope = Arc::new(ParamScope::root(&netlist.params).unwrap());
    let expanded = expand_subcircuits(&netlist, &scope, SubcircuitLimits::default()).unwrap();
    assert_eq!(expanded.devices, netlist.devices);
    assert_eq!(expanded.models, netlist.models);
}
