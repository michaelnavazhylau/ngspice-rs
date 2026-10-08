//! Behavioural sources (GitHub #79): the B card and nonlinear E/G/F/H
//! grammars, positioned errors, writer round trips, front-end resolution of
//! `.param`/`.func`/numparam values and the `inpcom.c` lowering passes.
use std::path::Path;
use std::sync::Arc;

use spice_core::SpiceError;
use spice_netlist::ast::{DeviceInstance, Netlist, ParameterKind};
use spice_netlist::behavioural::{lower_nonlinear_sources, poly_exponents};
use spice_netlist::bexpr::{BExpr, BExprKind, BehaviouralExpression};
use spice_netlist::elaborate::literalize;
use spice_netlist::eval::{EvalBudget, ParamScope};
use spice_netlist::source::parse_deck_text;
use spice_netlist::{Parser, semantic_diff, write_netlist};

fn try_parse(body: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("in.cir"),
        &format!("behavioural\n{body}\n.end\n"),
    ))
}

fn parse(body: &str) -> Netlist {
    try_parse(body).unwrap_or_else(|error| panic!("{error}\n{body}"))
}

fn device(body: &str) -> DeviceInstance {
    parse(body).devices.last().unwrap().clone()
}

fn expression(device: &DeviceInstance, name: &str) -> BehaviouralExpression {
    let parameter = device
        .parameters
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("{} has no {name}", device.name));
    match &parameter.kind {
        ParameterKind::Behavioural(expression) => (**expression).clone(),
        other => panic!("{name} is {other:?}"),
    }
}

fn names(device: &DeviceInstance) -> Vec<&str> {
    device.parameters.iter().map(|p| p.name.as_str()).collect()
}

/// A compact rendering of a tree, groups dropped.
fn shape(expr: &BExpr) -> String {
    match &expr.kind {
        BExprKind::Number { value, .. } => format!("{value}"),
        BExprKind::Name(name) => name.clone(),
        BExprKind::Voltage { positive, negative } => match negative {
            Some(negative) => format!("v({positive},{negative})"),
            None => format!("v({positive})"),
        },
        BExprKind::Current(name) => format!("i({name})"),
        BExprKind::Unary { op, operand } => format!("({op:?} {})", shape(operand)),
        BExprKind::Binary { op, lhs, rhs } => {
            format!("({} {} {})", shape(lhs), op.symbol(), shape(rhs))
        }
        BExprKind::Ternary {
            condition,
            then,
            otherwise,
        } => format!(
            "({} ? {} : {})",
            shape(condition),
            shape(then),
            shape(otherwise)
        ),
        BExprKind::Call { name, arguments } => format!(
            "{name}[{}]",
            arguments.iter().map(shape).collect::<Vec<_>>().join(",")
        ),
        BExprKind::Group(inner) => shape(inner),
        BExprKind::Value(value) => format!("{{{}}}", value.text),
        BExprKind::Table(table) => format!("table({}, {})", shape(&table.input), table.domain),
    }
}

#[test]
fn b_cards_parse_with_c_precedence_and_setters() {
    let b = device("B1 Out GND v = 2*V(In)^2 - -3**2 + i(Vin) m=2 tc1=1m");
    assert_eq!((b.name.as_str(), b.designator), ("b1", 'b'));
    assert_eq!(b.nodes, ["out", "0"]);
    assert_eq!(names(&b), ["v", "m", "tc1"]);
    let v = expression(&b, "v");
    assert_eq!(v.text, "2*V(In)^2 - -3**2 + i(Vin)");
    assert!(!v.verbatim);
    assert_eq!(
        shape(&v.root),
        "(((2 * (v(in) ^ 2)) - (Minus (3 ^ 2))) + i(vin))"
    );
    // The expression starts after `=`; the setter keeps its own position.
    assert_eq!(v.span.start.column, 16);
    assert_eq!(b.parameters[1].location.column, 43);

    let i = device("b2 0 o i={ v(a,b) > 0 ? 1m : 0 }");
    assert_eq!(
        shape(&expression(&i, "i").root),
        "((v(a,b) > 0) ? 0.001 : 0)"
    );
    // Braces and quotes are whitespace for C (inp_modify_exp), not groups.
    let quoted = device("b3 o 0 v='2*{1+1}'");
    assert_eq!(shape(&expression(&quoted, "v").root), "((2 * 1) + 1)");
    // A `=pwl(` card keeps numparam values in braces.
    let pwl = device("b4 o 0 v=pwl(v(in), 0, 0, {k*2}, 1)");
    let pwl = expression(&pwl, "v");
    assert!(pwl.verbatim);
    assert_eq!(shape(&pwl.root), "pwl[v(in),0,0,{k*2},1]");
}

#[test]
fn malformed_b_cards_are_positioned_errors() {
    for (body, message, column) in [
        ("b1 o 0 v=2*(v(in)+1", "expected ')'", 20),
        ("b1 o 0 v=2*v(in) 3", "unexpected '3'", 18),
        ("b1 o 0 v=1 i=2", "exactly one v= or i=", 12),
        ("b1 o 0 x=1", "expected v=expression", 8),
        ("b1 o 0 v=1 m", "unexpected 'm'", 12),
        ("b1 o 0 v=", "empty behavioural expression", 10),
        ("b1 o 0 v=v (a)", "without a space", 10),
        ("b1 o 0 v=i(a,b)", "i() takes one source name", 13),
        ("b1 o 0 v=1 = 2", "unexpected '='", 12),
    ] {
        let error = try_parse(body).unwrap_err();
        match &error {
            SpiceError::Parse {
                location,
                message: text,
            } => {
                assert!(text.contains(message), "{body}: {text}");
                assert_eq!(location.column, column, "{body}: {text}");
            }
            other => panic!("{body}: {other}"),
        }
    }
}

#[test]
fn nonlinear_controlled_forms_parse_into_ordered_setters() {
    let e = device("e1 o 0 value = {v(a)*2} m=3");
    assert_eq!(names(&e), ["value", "m"]);
    assert_eq!(e.nodes, ["o", "0"]);
    assert_eq!(expression(&e, "value").text, "{v(a)*2}");
    let g = device("g1 0 o cur='v(a)' m=2");
    assert_eq!(names(&g), ["value", "m"]);
    let table = device("g2 0 o TABLE {v(a)*2} = (-1,-1m) (0,0) (2,{k}) m=2");
    assert_eq!(names(&table), ["table", "x", "y", "x", "y", "x", "y", "m"]);
    assert!(matches!(
        table.parameters[6].kind,
        ParameterKind::Expression(_)
    ));
    let ltspice = device("e2 o 0 a b table=(0, 0, 1, 2)");
    assert_eq!(ltspice.nodes, ["o", "0", "a", "b"]);
    assert_eq!(names(&ltspice), ["table", "x", "y", "x", "y"]);
    assert!(matches!(ltspice.parameters[0].kind, ParameterKind::Flag));
    let poly = device("e3 o 0 poly(2) a 0 (b, 0) 1 2 3");
    assert_eq!(poly.nodes, ["o", "0", "a", "0", "b", "0"]);
    assert_eq!(names(&poly), ["poly", "coef", "coef", "coef"]);
    let fpoly = device("f1 0 o poly(2) v1 v2 0 1 1 m=2");
    assert_eq!(
        names(&fpoly),
        ["poly", "control", "control", "coef", "coef", "coef", "m"]
    );
    // inp_poly_2g6_compat: values after the gain make an implicit POLY(1)
    // whose first coefficient is the "gain".
    let implicit = device("e4 o 0 a b 0.5 1 0.25");
    assert_eq!(names(&implicit), ["poly", "coef", "coef", "coef"]);
    assert_eq!(implicit.parameters[0].value, "1");
    let implicit = device("h1 o 0 vx 1 2");
    assert_eq!(names(&implicit), ["poly", "control", "coef", "coef"]);
}

#[test]
fn malformed_nonlinear_forms_are_explicit_errors() {
    for (body, message) in [
        ("e1 o 0 poly(1) a 0", "at least one coefficient"),
        ("e1 o 0 poly(0) a 0 1", "positive integer POLY dimension"),
        ("e1 o 0 poly(1) a 0 1 2 m=2", "remove it"),
        ("e1 o 0 table {v(a)} = (0,0) (1)", "pairs"),
        ("e1 o 0 table (0,0) (1,1)", "in braces"),
        (
            "e1 o 0 table {v(a)} = (0,0) (1,1) m=2",
            "not accepted on an E TABLE",
        ),
        ("g1 o 0 a b table=(0,0,1,1)", "four-node G TABLE"),
        ("f1 o 0 value={v(a)}", "not a form"),
        ("g1 o 0 vol={v(a)}", "not a form"),
        ("e1 o 0 value={v(a)} junk", "unexpected 'junk'"),
    ] {
        let error = try_parse(body).unwrap_err();
        assert!(error.to_string().contains(message), "{body}: {error}");
    }
    let error = try_parse("e1 o 0 laplace {v(a)} = {1/(1+s)}").unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
}

fn round_trip(body: &str) -> String {
    let first = parse(body);
    let written = write_netlist(&first).unwrap();
    let second = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("out.cir"), &written))
        .unwrap_or_else(|error| panic!("{error}\n{written}"));
    if let Some(difference) = semantic_diff(&first, &second) {
        panic!("not equivalent: {difference}\n{written}");
    }
    assert_eq!(write_netlist(&second).unwrap(), written, "fixed point");
    written
}

#[test]
fn the_writer_round_trips_every_behavioural_form() {
    let written = round_trip(
        "b1 o 0 v = 2*v(in)^2 + {k} m=2 temp=30\nb2 0 o i='i(v1)*v(a, b) > 0 ? 1 : 2'\n\
         b3 o 0 v=pwl(time, 0, 0, {t1}, 1)\ne1 o 0 vol={v(a)*2} m=2\n\
         g1 0 o value={1m*v(a)} m=3\ne2 o 0 table {v(a)} = (0,0) (1,{k})\n\
         g2 0 o table {2*v(a)} (0,0) (1,1) m=2\ne3 o 0 a b table=(0, 0, 1, 2)\n\
         e4 o 0 poly(2) a 0 b 0 1 2 3\nf1 0 o poly(1) v1 0 1 m=2\nh1 o 0 vx 1 2",
    );
    for line in [
        "b1 o 0 v=2*v(in)^2 + {k} m=2 temp=30",
        "b2 0 o i='i(v1)*v(a, b) > 0 ? 1 : 2'",
        "b3 o 0 v=pwl(time, 0, 0, {t1}, 1)",
        "e1 o 0 value={v(a)*2} m=2",
        "g1 0 o value={1m*v(a)} m=3",
        "e2 o 0 table {v(a)} = (0, 0) (1, {k})",
        "g2 0 o table {2*v(a)} = (0, 0) (1, 1) m=2",
        "e3 o 0 a b table=(0, 0, 1, 2)",
        "e4 o 0 poly(2) a 0 b 0 1 2 3",
        "f1 0 o poly(1) v1 0 1 m=2",
        "h1 o 0 poly(1) vx 1 2",
    ] {
        assert!(written.lines().any(|l| l == line), "{line}\n{written}");
    }
}

fn resolved(body: &str) -> Netlist {
    literalize(&parse(body)).unwrap().netlist
}

#[test]
fn resolution_substitutes_parameters_and_expands_functions() {
    let netlist = resolved(
        ".param k=3 vt=0.025\n.func soft(x) {tanh(x)*k}\n.func vd(a) {v(a)-v(b)*a}\n\
         b1 o 0 v=k*v(in) + soft(1+1) + vd(2) + 1.23456789012345 + pi + time",
    );
    let v = expression(&netlist.devices[0], "v");
    // `.param` values are exact, the literal keeps C's 11 digits, the .func
    // argument is parenthesised and v(a) in a body names node `a` literally.
    assert_eq!(
        shape(&v.root),
        "((((((3 * v(in)) + (tanh[(1 + 1)] * 3)) + (v(a) - (v(b) * 2))) + 1.2345678901) + pi) \
         + time)"
    );
}

#[test]
fn verbatim_lines_evaluate_numparam_values_and_keep_full_precision() {
    let netlist = resolved(".param k=2\nb1 o 0 v=pwl(v(in), 0, 0, {k*1.5}, 1.23456789012345)");
    let v = expression(&netlist.devices[0], "v");
    assert_eq!(shape(&v.root), "pwl[v(in),0,0,3,1.23456789012345]");
    // C leaves bare names unsubstituted on such a line.
    let error = literalize(&parse(".param k=2\nb1 o 0 v=pwl(v(in), 0, 0, k, 1)")).unwrap_err();
    assert!(error.to_string().contains("bare name 'k'"), "{error}");
}

#[test]
fn resolution_errors_name_their_cause() {
    for (body, message) in [
        ("b1 o 0 v=undefinedp*2", "undefined parameter [undefinedp]"),
        ("b1 o 0 v=sqr(2)", "no such function 'sqr'"),
        (".func f(x) {x}\nb1 o 0 v=f(1, 2)", "takes 1 argument"),
    ] {
        let error = literalize(&parse(body)).unwrap_err();
        assert!(error.to_string().contains(message), "{body}: {error}");
    }
    // A .func may redefine a built-in function, as in C's macro expansion.
    let netlist = resolved(".func sin(x) {x*2}\nb1 o 0 v=sin(v(a))");
    assert_eq!(
        shape(&expression(&netlist.devices[0], "v").root),
        "(v(a) * 2)"
    );
}

#[test]
fn statistical_functions_are_not_yet_ported_at_their_call() {
    // C's inp.c eval_agauss() replaces these by a drawn value before parsing
    // a B line (and E/G VALUE lines become B lines first).
    for (body, column) in [
        ("b1 o 0 v=agauss(1,0.1,3)*v(1)", 10),
        ("b1 o 0 v=2*gauss(1,0.1,3)", 12),
        ("b1 o 0 i=aunif(1,0.1)*1m", 10),
        ("e1 o 0 value={unif(1,0.1)*v(1)}", 15),
        ("g1 o 0 value={limit(1,0.1)*v(1)}", 15),
    ] {
        let netlist = lower_nonlinear_sources(&parse(body)).unwrap();
        let error = literalize(&netlist).unwrap_err();
        assert!(error.is_not_yet_ported(), "{body}: {error}");
        let text = error.to_string();
        assert!(
            text.contains(&format!("in.cir:2:{column}:")),
            "{body}: {text}"
        );
        assert!(text.contains("src/frontend/inp.c (eval_agauss)"), "{text}");
    }
    // A user .func of the same name is expanded first, as in C.
    let netlist = resolved(".func agauss(a,b,c) {a*2}\nb1 o 0 v=agauss(1,0.1,3)*v(x)");
    assert_eq!(
        shape(&expression(&netlist.devices[0], "v").root),
        "((1 * 2) * v(x))"
    );
}

#[test]
fn resolution_against_an_explicit_scope() {
    let netlist = parse(".param a=4\nb1 o 0 v=a*v(x)");
    let scope = ParamScope::for_netlist(&netlist).unwrap();
    let mut budget = EvalBudget::default();
    let v = expression(&netlist.devices[0], "v");
    let resolved =
        spice_netlist::behavioural::resolve_expression(&v, &Arc::new(scope), &mut budget).unwrap();
    assert_eq!(shape(&resolved.root), "(4 * v(x))");
}

fn lowered(body: &str) -> Netlist {
    lower_nonlinear_sources(&parse(body)).unwrap()
}

fn summary(netlist: &Netlist) -> Vec<String> {
    netlist
        .devices
        .iter()
        .map(|d| {
            format!(
                "{} {} [{}]",
                d.name,
                d.nodes.join(" "),
                d.parameters
                    .iter()
                    .map(|p| format!("{}={}", p.name, p.value))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        })
        .collect()
}

#[test]
fn value_forms_lower_onto_a_b_source_and_an_internal_node() {
    let netlist = lowered("e1 o 0 value={v(a)*2} m=3\ng1 0 p cur={v(a)} m=2");
    assert_eq!(
        summary(&netlist),
        [
            "e1 o 0 e1_int1 0 [gain=1]",
            "be1 e1_int1 0 [v={v(a)*2} m=3]",
            "g1 0 p g1_int1 0 [gain=2]",
            "bg1 g1_int1 0 [v={v(a)}]",
        ]
    );
}

#[test]
fn table_forms_lower_onto_the_xspice_pwl_transfer() {
    let netlist = lowered(
        "e1 o 0 table {v(a)} = (0,0) (1,2)\ng[1] 0 p table {v(a)} = (0, 3m) m=2\n\
         e2 q 0 a b table=(0, 0, 1, 1)",
    );
    let lines = summary(&netlist);
    assert_eq!(lines[0], "e1 o 0 e1_int1 0 [gain=1]");
    assert_eq!(lines[1], "be1 e1_int2 0 [v=v(a)]");
    assert!(lines[2].starts_with("ae1 e1_int1 0 [v=xspice-pwl input_domain=0.1"));
    // One pair is a constant source; G names lose `[`/`]` (inp_compat).
    assert_eq!(lines[3], "g[1] 0 p g_1__int1 0 [gain=2]");
    assert_eq!(lines[4], "vg[1] g_1__int1 0 [dc=3m]");
    assert_eq!(lines[5], "e2 q 0 e2_int1 0 [gain=1]");
    assert!(lines[6].starts_with("ae2 e2_int1 0 [v=xspice-pwl input_domain=0.001"));
    let ParameterKind::Behavioural(transfer) = &netlist.devices[6].parameters[0].kind else {
        panic!("not behavioural");
    };
    assert_eq!(shape(&transfer.root), "table(v(a,b), 0.001)");
}

#[test]
fn poly_forms_lower_onto_spice2poly_in_spice2_term_order() {
    let netlist = lowered("e1 o 0 poly(2) a 0 b 0 1 2 3 4 5 6\nf1 0 p poly(1) vx 0 2 m=3");
    assert_eq!(netlist.devices[0].name, "a$poly$e1");
    assert_eq!(netlist.devices[0].designator, 'a');
    assert_eq!(netlist.devices[0].nodes, ["o", "0"]);
    let v = expression(&netlist.devices[0], "v");
    assert_eq!(
        shape(&v.root),
        "(((((1 + (2 * v(a,0))) + (3 * v(b,0))) + (4 * (v(a,0) * v(a,0)))) + \
         (5 * (v(a,0) * v(b,0)))) + (6 * (v(b,0) * v(b,0))))"
    );
    assert_eq!(names(&netlist.devices[1]), ["i", "m"]);
    assert_eq!(
        shape(&expression(&netlist.devices[1], "i").root),
        "(0 + (2 * i(vx)))"
    );
    assert_eq!(
        poly_exponents(2, 3),
        vec![vec![1, 0], vec![0, 1], vec![2, 0]]
    );
}

#[test]
fn current_references_insert_measurement_sources_like_inp_meas_current() {
    let netlist = lowered(
        "b1 o 0 v=1\nb2 p 0 v=i(b1)*2 + i(b1)\nb3 q 0 v=max(1,i(b1))\n\
         e1 r 0 a 0 2\nb4 s 0 v=i(e1) + i(vx)",
    );
    let lines = summary(&netlist);
    // Both i(b1) of b2 become i(v_b1); b1's first node moves to o_vmeas_0
    // behind one zero-volt source (the serial still counts both).
    assert_eq!(lines[0], "b1 o_vmeas_0 0 [v=1]");
    assert_eq!(lines[1], "v_b1 o o_vmeas_0 [dc=0]");
    assert_eq!(
        shape(&expression(&netlist.devices[2], "v").root),
        "((i(v_b1) * 2) + i(v_b1))"
    );
    // After a comma C does not rewrite; a simple E and a V keep theirs.
    assert_eq!(
        shape(&expression(&netlist.devices[3], "v").root),
        "max[1,i(b1)]"
    );
    assert_eq!(
        shape(&expression(&netlist.devices[5], "v").root),
        "(i(e1) + i(vx))"
    );
}
