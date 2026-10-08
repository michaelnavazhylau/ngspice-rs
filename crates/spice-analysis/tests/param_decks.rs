//! #15: top-level `.param` decks produce the same results as literal decks
//! through the production RunConfig/Circuit/runner APIs.
use spice_analysis::{RunConfig, runner};
use spice_core::SpiceError;
use spice_netlist::{Parser, ast::Netlist, source::parse_deck_text};
use std::path::Path;

fn deck(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("p.cir"),
            &format!("t\n{body}\n.end\n"),
        ))
        .unwrap()
}

fn run_first(body: &str) -> Result<String, SpiceError> {
    let n = deck(body);
    let config = RunConfig::from_netlist(&n)?;
    let mut circuit = config.circuit(&n)?;
    let card = &n.analyses[0];
    let request = config.request_for(card)?;
    let plot = runner(card.kind)?.run(&mut circuit, &request, &config.context())?;
    Ok(format!("{plot:?}"))
}

fn same(param: &str, literal: &str) {
    let a = run_first(param).unwrap();
    let b = run_first(literal).unwrap();
    assert_eq!(a, b, "{param}\nvs\n{literal}");
}

#[test]
fn op_with_param_resistor_matches_literal() {
    same(
        ".param rtop=1k ratio=3\nv1 in 0 dc {2*2.5}\nr1 in out {rtop}\nr2 out 0 {rtop*ratio}\n.op",
        "v1 in 0 dc 5\nr1 in out 1000\nr2 out 0 3000\n.op",
    );
}

#[test]
fn forward_reference_and_redefinition_follow_c_rules() {
    // b is defined before a (hoisting); the later a=2k wins, a=1k is dropped.
    same(
        ".param b={a*2}\n.param a=1k\n.param a=2k\nv1 in 0 dc 1\nr1 in 0 {b}\n.op",
        "v1 in 0 dc 1\nr1 in 0 4000\n.op",
    );
}

#[test]
fn dc_ac_and_tran_arguments_match_literals() {
    same(
        ".param lo=0 hi=2 st=0.5\nv1 a 0 dc 0\nr1 a 0 1k\n.dc v1 {lo} {hi} {st}",
        "v1 a 0 dc 0\nr1 a 0 1k\n.dc v1 0 2 0.5",
    );
    same(
        ".param f1=10 f2={f1*1k} n=5\nv1 a 0 dc 0 ac 1\nr1 a b 1k\nc1 b 0 {1u*2}\n.ac dec {n} {f1} {f2}",
        "v1 a 0 dc 0 ac 1\nr1 a b 1k\nc1 b 0 2e-6\n.ac dec 5 10 10000",
    );
    same(
        ".param tau=1m\nv1 in 0 dc 1\nr1 in out 1k\nc1 out 0 {tau/1k}\n.tran {tau/10} {tau*5} backend=diffsol method=bdf",
        "v1 in 0 dc 1\nr1 in out 1k\nc1 out 0 1e-6\n.tran 1e-4 5e-3 backend=diffsol method=bdf",
    );
}

#[test]
fn model_parameters_are_literalized() {
    same(
        ".param rs=100\nv1 a 0 1\nr1 a 0 rm\n.model rm r(r={rs*10} tc1=0)\n.op",
        "v1 a 0 1\nr1 a 0 rm\n.model rm r(r=1000 tc1=0)\n.op",
    );
}

#[test]
fn failures_are_explicit_with_source_context() {
    let e = run_first(".param a={1/0}\nr1 a 0 1k\n.op").unwrap_err();
    assert!(e.to_string().contains("division by zero"), "{e}");
    assert!(e.to_string().contains("p.cir:2:"), "{e}");
    let e = run_first("v1 a 0 dc {nope}\nr1 a 0 1k\n.op").unwrap_err();
    assert!(e.to_string().contains("undefined parameter 'nope'"), "{e}");
    // An analysis expression is not silently dropped without a scope.
    let n = deck("v1 a 0 1\nr1 a 0 1k\n.dc v1 0 {1+1} 1");
    let c = RunConfig::from_options(&n.options, &Default::default()).unwrap();
    assert!(matches!(
        c.request_for(&n.analyses[0]),
        Err(SpiceError::Unsupported { .. })
    ));
}

#[test]
fn subcircuits_are_elaborated_only_when_instantiated() {
    // An unused definition does not block a flat deck and its body never runs.
    let n = deck(".param a=1\n.subckt s x y\nr1 x y {a}\n.ends\nr9 n 0 1k");
    let circuit = RunConfig::from_netlist(&n).unwrap().circuit(&n).unwrap();
    assert_eq!(circuit.device_count(), 1);
    // An instantiated one binds its body against the caller's scope (#18).
    let n = deck(".param rv=2k\n.subckt s x y\nr1 x y {rv}\n.ends\nv1 n 0 1\nx1 n m s\nr9 m 0 1k");
    let circuit = RunConfig::from_netlist(&n).unwrap().circuit(&n).unwrap();
    assert_eq!(circuit.resistor("r.x1.r1").unwrap().1.supplied, 2e3);
    assert_eq!(circuit.device_count(), 3);
}

#[test]
fn func_and_quoted_values_reach_devices_analyses_and_initial_conditions() {
    // #107: `.func` calls and single-quoted values through RunConfig: device
    // values, analysis arguments and `.ic` entries.
    same(
        ".func half(x) {x/2}\n.param lo=0\nv1 a 0 dc 0\nr1 a 0 'half(2k)'\n\
         .dc v1 'lo' {half(4)} 'half(1)'",
        "v1 a 0 dc 0\nr1 a 0 1k\n.dc v1 0 2 0.5",
    );
    same(
        ".func tau(r, c) {r*c}\nv1 a 0 dc 1\nr1 a b 1k\nc1 b 0 1u\n.ic v(b)='tau(1k,1u)*1k'\n\
         .tran 'tau(1k,1u)/10' {tau(1k,1u)*2}",
        "v1 a 0 dc 1\nr1 a b 1k\nc1 b 0 1u\n.ic v(b)=1\n.tran 1e-4 2e-3",
    );
}
