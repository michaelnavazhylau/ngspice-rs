//! #107: `.func` scoping through subcircuit expansion and single-quoted
//! instance/body values. The rules were probed against the C binary
//! (`inpcom.c` `inp_expand_macros_in_deck()`); the `func_quotes` golden
//! re-checks them through a full simulation.
use std::path::Path;

use spice_core::{Real, SpiceError};
use spice_devices::Circuit;
use spice_netlist::{Parser, source::parse_deck_text};

fn circuit(body: &str) -> Result<Circuit, SpiceError> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(
        Path::new("funcs.cir"),
        &format!("functions\n{body}\n.end\n"),
    ))?;
    Circuit::from_netlist(&netlist)
}

fn supplied(circuit: &Circuit, name: &str) -> Real {
    circuit.resistor(name).expect("resistor exists").1.supplied
}

#[test]
fn body_functions_shadow_the_deck_and_stay_local() {
    let c = circuit(
        ".func f(x) {x*2}\n\
         .subckt s a\n.func f(x) {x*5}\nr1 a 0 {f(1)}\n.ends\n\
         .subckt t a\nr1 a 0 {f(1)}\n.ends\n\
         x1 n s\nx2 n t\nr0 n 0 {f(1)}\nv1 n 0 dc 1",
    )
    .unwrap();
    assert_eq!(supplied(&c, "r.x1.r1"), 5.0, "local definition wins");
    assert_eq!(supplied(&c, "r.x2.r1"), 2.0, "deck definition");
    assert_eq!(
        supplied(&c, "r0"),
        2.0,
        "body definition is invisible outside"
    );
}

#[test]
fn free_names_in_a_body_resolve_in_the_instance_scope() {
    let c = circuit(
        ".param k=2\n.func f(x) {x*k}\n\
         .subckt s a w=4\n.param k=3\nr1 a 0 {f(1)}\nr2 a 0 'f(w)'\n.ends\n\
         x1 n s w=7\nr0 n 0 {f(1)}\nv1 n 0 dc 1",
    )
    .unwrap();
    assert_eq!(supplied(&c, "r.x1.r1"), 3.0);
    assert_eq!(supplied(&c, "r.x1.r2"), 21.0);
    assert_eq!(supplied(&c, "r0"), 2.0);
}

#[test]
fn functions_are_lexical_not_inherited_from_the_instantiating_body() {
    // `inner` is instantiated from `outer`, whose local `g` must not leak into
    // it; C scopes functions by the definition nesting.
    let error = circuit(
        ".subckt inner a\nr1 a 0 {g(1)}\n.ends\n\
         .subckt outer a\n.func g(x) {x}\nx1 a inner\n.ends\n\
         x1 n outer\nv1 n 0 dc 1",
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("undefined function 'g'"),
        "{error}"
    );
    assert!(error.to_string().contains("funcs.cir:3:9"), "{error}");
}

#[test]
fn functions_reach_formals_instance_values_and_body_params() {
    let c = circuit(
        ".func f(x) {x*2}\n\
         .subckt s a w={f(3)}\n.func h(y) 'y+w'\n.param v={h(1)}\nr1 a 0 {v}\n.ends\n\
         x1 n s\nx2 n s w='f(5)'\nv1 n 0 dc 1",
    )
    .unwrap();
    assert_eq!(supplied(&c, "r.x1.r1"), 7.0);
    assert_eq!(supplied(&c, "r.x2.r1"), 11.0);
}

#[test]
fn errors_inside_bodies_keep_their_locations() {
    let error = circuit(".subckt s a\n.func h(y) {y}\nr1 a 0 {h(1,2)}\n.ends\nx1 n s\nv1 n 0 dc 1")
        .unwrap_err();
    let text = error.to_string();
    assert!(text.contains("funcs.cir:4:9"), "{text}");
    assert!(text.contains("takes 1 argument(s), found 2"), "{text}");
}

#[test]
fn invalid_body_functions_fail_only_when_instantiated() {
    // C checks a body's .func cards when the subcircuit is expanded.
    for body in [
        ".subckt s a\n.func h(x) {h(x)}\nr1 a 0 1k\n.ends",
        ".subckt s a\n.func g(x) {x}\n.func h(x) {g(x,1)}\nr1 a 0 1k\n.ends",
    ] {
        circuit(&format!("{body}\nv1 n 0 dc 1\nr2 n 0 1k"))
            .unwrap_or_else(|e| panic!("{body}: {e}"));
        let error = circuit(&format!("{body}\nx1 n s\nv1 n 0 dc 1")).unwrap_err();
        let text = error.to_string();
        assert!(!error.is_not_yet_ported(), "{text}");
        assert!(
            text.contains("recursive .func definition") || text.contains("found 2"),
            "{text}"
        );
    }
}
