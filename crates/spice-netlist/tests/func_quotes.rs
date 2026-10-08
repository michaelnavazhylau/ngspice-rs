//! #107: `.func` definitions and single-quoted `'expr'` values.
//!
//! Positive cases follow behaviour probed against the C binary
//! (`c_func_eval.rs` re-checks the numeric ones live with `NGSPICE_BIN`).
use std::path::Path;
use std::sync::Arc;

use spice_core::SpiceError;
use spice_netlist::ast::{Netlist, NodeHintValue, ParameterKind, ScopedCardKind};
use spice_netlist::elaborate::literalize;
use spice_netlist::eval::{EvalBudget, FunctionScope, ParamScope};
use spice_netlist::expr::ExprKind;
use spice_netlist::{Parser, semantic_diff, semantic_eq, source::parse_deck_text, write_netlist};

fn parse(body: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("f.cir"),
        &format!("t\n{body}\n.end\n"),
    ))
}

fn deck(body: &str) -> Netlist {
    parse(body).unwrap_or_else(|e| panic!("{body}: {e}"))
}

/// The value of top-level parameter `name` (functions and params resolved).
fn value(body: &str, name: &str) -> f64 {
    ParamScope::for_netlist(&deck(body))
        .unwrap_or_else(|e| panic!("{body}: {e}"))
        .get(name)
        .unwrap()
}

fn probe(body: &str, expression: &str) -> f64 {
    value(
        &format!("{body}\n.param probe__={{{expression}}}"),
        "probe__",
    )
}

fn scope_error(body: &str) -> SpiceError {
    match parse(body) {
        Ok(netlist) => ParamScope::for_netlist(&netlist).expect_err(body),
        Err(error) => error,
    }
}

#[test]
fn func_cards_parse_with_positioned_formals_and_any_body_spelling() {
    let n = deck(
        ".func f(x) {x*2}\n.FUNC G( A , b )= 'a+b'\n.func h() 3\n.func k(y) y * 2 + 1\n.param p=1",
    );
    assert_eq!(n.functions.len(), 4);
    let names: Vec<_> = n.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["f", "g", "h", "k"]);
    let g = &n.functions[1];
    let formals: Vec<_> = g.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(formals, ["a", "b"]);
    assert_eq!(g.name_span.start.column, 7);
    assert_eq!(g.parameters[0].span.start.column, 10);
    assert_eq!(g.parameters[1].span.start.column, 14);
    assert!(g.body.braced && g.body.quoted);
    assert_eq!(g.body.text, "a+b");
    assert_eq!(g.body.span.start.column, 20);
    let f = &n.functions[0];
    assert!(f.body.braced && !f.body.quoted);
    assert_eq!(n.functions[2].parameters.len(), 0);
    let k = &n.functions[3];
    assert!(!k.body.braced);
    assert_eq!(k.body.text, "y * 2 + 1");
    let kinds: Vec<_> = n.cards.iter().map(|c| c.kind).collect();
    assert_eq!(
        kinds,
        [
            ScopedCardKind::Func(0),
            ScopedCardKind::Func(1),
            ScopedCardKind::Func(2),
            ScopedCardKind::Func(3),
            ScopedCardKind::Param(0),
            ScopedCardKind::End,
        ]
    );
    // Body-local definitions stay in the body.
    let n = deck(".subckt s a\n.func f(x) {x}\nr1 a 0 {f(2)}\n.ends\n.func g(x) {x}");
    assert_eq!(n.functions.len(), 1);
    assert_eq!(n.subcircuits[0].functions.len(), 1);
    assert_eq!(n.subcircuits[0].cards[0].kind, ScopedCardKind::Func(0));
}

#[test]
fn malformed_func_cards_are_positioned_errors() {
    for (body, column, message) in [
        (".func", 6, "expected a function name"),
        (".func f", 8, "expected '('"),
        (".func f x {x}", 9, "expected '('"),
        (".func f(x {x}", 11, "expected ',' or ')'"),
        (".func f(x,) {x}", 11, "expected a parameter name"),
        (".func f(x)", 11, "expected a body"),
        (".func f(x) {x*}", 15, "expected an operand"),
        (".func f(x) {x+{1}}", 15, "nested braces"),
        (".func f(max) {1}", 9, "name of a built-in function"),
        (".param f(x", 11, "expected ',' or ')'"),
        (".param f(x)=", 13, "expected a body"),
        (
            ".param f(x) {x}",
            13,
            "expected '=' after the parameter list",
        ),
    ] {
        match parse(body).expect_err(body) {
            SpiceError::Parse {
                location,
                message: m,
            } => {
                assert_eq!((location.line, location.column), (2, column), "{body}: {m}");
                assert!(m.contains(message), "{body}: {m}");
            }
            other => panic!("{body}: expected Parse, got {other}"),
        }
    }
    // Different-arity redefinition of an allowlisted built-in is valid C the
    // port cannot represent yet; double-quoted bodies are strings.
    for body in [".func max(a) {a}", ".func f(x) \"x\""] {
        assert!(parse(body).unwrap_err().is_not_yet_ported(), "{body}");
    }
    // Forms C accepts that the port does not reproduce: glued pieces after a
    // delimited body (`inp_strip_braces()`), duplicate formals (C binds the
    // first), and a function definition inside a multi-assignment `.param`
    // card (C splits the card first). Positioned NotYetPorted, not Parse.
    for (body, position, message) in [
        (
            ".func f(x) {x}+{1}",
            "f.cir:2:15",
            "text after the delimited body",
        ),
        (
            ".func f(x) {x} y",
            "f.cir:2:16",
            "text after the delimited body",
        ),
        (
            ".param f(x)={x} a=2",
            "f.cir:2:17",
            "text after the delimited body",
        ),
        (".func f(x,x) {x}", "f.cir:2:11", "duplicate parameter 'x'"),
        (
            ".param a=1 f(x)={x}",
            "f.cir:2:13",
            "inside a multi-assignment .param",
        ),
    ] {
        let error = parse(body).expect_err(body);
        assert!(error.is_not_yet_ported(), "{body}: {error}");
        let text = error.to_string();
        assert!(
            text.contains(position) && text.contains(message),
            "{body}: {text}"
        );
    }
}

#[test]
fn param_cards_spelling_a_function_are_func_definitions() {
    // C: inpcom.c inp_fix_macro_param_func_paren_io() rewrites a `.param`
    // card whose first token contains '(' into `.func`, unconditionally.
    let n =
        deck(".param f(x)={x*3}\n.PARAM G(x, y) = 'x+y'\n.param h()=4\n.param p={f(3)+g(3,4)+h()}");
    assert_eq!(n.functions.len(), 3);
    assert_eq!(n.params.len(), 1);
    assert!(
        n.functions
            .iter()
            .all(|f| f.spelling == spice_netlist::ast::FuncSpelling::Param)
    );
    let g = &n.functions[1];
    assert_eq!(g.name, "g");
    assert_eq!(g.name_span.start.column, 8);
    assert!(g.body.quoted);
    assert_eq!(
        n.cards.iter().map(|c| c.kind).take(4).collect::<Vec<_>>(),
        [
            ScopedCardKind::Func(0),
            ScopedCardKind::Func(1),
            ScopedCardKind::Func(2),
            ScopedCardKind::Param(0),
        ]
    );
    assert_eq!(value(".param f(x)={x*3}\n.param p={f(3)}", "p"), 9.0);
    assert_eq!(
        probe(
            ".param f(x)={x*3}\n.param g(x,y) = 'x+y'\n.param h()=4",
            "f(3)+g(3,4)+h()"
        ),
        20.0
    );
    // The same definition as `.func`, semantically.
    assert!(semantic_eq(
        &deck(".param f(x)={x*3}"),
        &deck(".func f(x) {x*3}")
    ));
    // Inside a subcircuit body too.
    let n = deck(".subckt s a\n.param k(x)={x*2}\nr1 a 0 {k(2)}\n.ends");
    assert_eq!(n.subcircuits[0].functions.len(), 1);
}

#[test]
fn param_dependency_scan_is_bounded() {
    // Exponential DAG: each level calls the previous one twice. The
    // dependency scan memoizes free names, so only evaluation itself runs
    // into the node budget (C expands it textually and is no better).
    let mut body = String::from(".func f0(x) {x}\n");
    for i in 1..=60 {
        body.push_str(&format!(".func f{i}(x) {{f{}(x)+f{}(x)}}\n", i - 1, i - 1));
    }
    body.push_str(".param p={f60(1)}");
    let start = std::time::Instant::now();
    let error = scope_error(&body).to_string();
    assert!(error.contains("budget"), "{error}");
    assert!(start.elapsed() < std::time::Duration::from_secs(30));
    // A small DAG still resolves, with free names found through it.
    let mut body = String::from(".func f0(x) {x*k}\n");
    for i in 1..=15 {
        body.push_str(&format!(
            ".func f{i}(x) {{f{}(x)-f{}(x)+x}}\n",
            i - 1,
            i - 1
        ));
    }
    body.push_str(".param p={f15(2)}\n.param k=5");
    assert_eq!(value(&body, "p"), 2.0);
    // Deep linear chain on the `.param` path: a positioned depth error, not
    // a stack overflow.
    let mut body = String::from(".func f0(x) {x}\n");
    for i in 1..=20_000 {
        body.push_str(&format!(".func f{i}(x) {{f{}(x)}}\n", i - 1));
    }
    body.push_str(".param p={f20000(1)}");
    let error = scope_error(&body).to_string();
    assert!(error.contains("deeper than 1024 levels"), "{error}");
    assert!(
        error.contains("while collecting the dependencies of parameter 'p'"),
        "{error}"
    );
}

#[test]
fn calls_bind_formals_by_value_and_free_names_at_the_call_site() {
    assert_eq!(probe(".func f(x) {x*2}", "f(3)"), 6.0);
    assert_eq!(probe(".func f(x) {x*2}", "F(3)+1"), 7.0);
    assert_eq!(probe(".func f(x) x*2", "f(1+2)"), 6.0);
    assert_eq!(probe(".func f(x)=x*2", "f(3)"), 6.0);
    assert_eq!(probe(".func f() {5}", "f()+1"), 6.0);
    assert_eq!(probe(".func f(x,y) {x*y+k}\n.param k=10", "f(3,4)"), 22.0);
    // A formal shadows a parameter of the same name inside the body only.
    assert_eq!(probe(".func f(x) {x*2}\n.param x=100", "f(3)+x"), 106.0);
    // A parameter and a function may share a name.
    assert_eq!(probe(".param f=10\n.func f(x) {x+1}", "f(3)+f"), 14.0);
    // Bodies may call functions defined later; calls nest.
    assert_eq!(
        probe(".func f(x) {g(x)+1}\n.func g(y) {y*10}", "f(3)"),
        31.0
    );
    assert_eq!(probe(".func f(x) {x*2}", "f(f(2))"), 8.0);
    // C's textual expansion lets an enclosing call's formal capture a
    // callee's free name.
    assert_eq!(
        probe(".func g(y) {y+x}\n.func f(x) {g(1)}\n.param x=100", "f(5)"),
        6.0
    );
    assert_eq!(probe(".func g(y) {y+x}\n.param x=100", "g(1)"), 101.0);
}

#[test]
fn definitions_hoist_shadow_and_last_one_wins() {
    assert_eq!(probe(".func f(x) {x*2}\n.func f(x) {x*3}", "f(3)"), 9.0);
    // A .param may use a function and a parameter defined after it.
    assert_eq!(
        value(".param p={f(2)}\n.func f(x) {x*q}\n.param q=3", "p"),
        6.0
    );
    // A .func replaces a built-in of the same arity (C expands it first),
    // also inside other bodies.
    assert_eq!(probe(".func max(a,b) {a+b}", "max(3,4)"), 7.0);
    assert_eq!(
        probe(".func max(a,b) {a+b}\n.func f(x) {max(x,1)}", "f(3)"),
        4.0
    );
    // Numparam built-ins outside the allowlist can be user-defined.
    assert_eq!(
        probe(".func limit(x,a,b) {min(max(x,a),b)}", "limit(5,1,3)"),
        3.0
    );
}

#[test]
fn function_errors_are_explicit() {
    // Wrong arity at a site.
    let e = scope_error(".func f(x) {x*2}\n.param p={f(3,4)}").to_string();
    assert!(e.contains("f.cir:3:11"), "{e}");
    assert!(e.contains("takes 1 argument(s), found 2"), "{e}");
    // Wrong arity inside an unused body is still an error (C aborts).
    let e = scope_error(".func f(x) {g(x,1)}\n.func g(x) {x}").to_string();
    assert!(e.contains("f.cir:2:13"), "{e}");
    assert!(
        e.contains("function 'g'") && e.contains("in the body of function 'f'"),
        "{e}"
    );
    // Undefined function.
    let e = scope_error(".param p={nope(1)}").to_string();
    assert!(
        e.contains("f.cir:2:11") && e.contains("undefined function 'nope'"),
        "{e}"
    );
    // Direct and mutual recursion, even unused.
    let e = scope_error(".func f(x) {f(x)}").to_string();
    assert!(
        e.contains("recursive .func definition: 'f' (f.cir:2:7) -> 'f'"),
        "{e}"
    );
    let e = scope_error(".func a(x) {b(x)+1}\n.func b(x) {c(x)}\n.func c(x) {a(x)}").to_string();
    assert!(
        e.contains("'a' (f.cir:2:7) -> 'b' (f.cir:3:7) -> 'c' (f.cir:4:7) -> 'a'"),
        "{e}"
    );
    // C checks a subcircuit's .func cards only when it is instantiated, so a
    // recursive or mis-called definition in an unused body is accepted (the
    // instantiated case is covered in spice-devices subckt_functions).
    for body in [
        ".subckt s a\n.func f(x) {f(x)}\n.ends",
        ".subckt s a\n.func g(x) {x}\n.func h(x) {g(x,1)}\n.ends",
    ] {
        let n = deck(body);
        assert!(ParamScope::for_netlist(&n).is_ok(), "{body}");
        assert!(literalize(&n).is_ok(), "{body}");
    }
    // Undefined names in a body are reported where the function is used,
    // with the call chain.
    let e = scope_error(".func f(x) {x*zz}\n.param p={f(1)}").to_string();
    assert!(e.contains("undefined parameter 'zz'"), "{e}");
    let n = deck(".func f(x) {x*zz}\nr1 a 0 {f(1)}");
    let e = literalize(&n).unwrap_err().to_string();
    assert!(
        e.contains("f.cir:2:15: undefined parameter 'zz'") && e.contains("called at f.cir:3:9"),
        "{e}"
    );
    // ... but an unused body with a free name is fine, as in C.
    assert_eq!(probe(".func f(x) {x*zz}", "1"), 1.0);
    // A self reference through a function is a self reference.
    let e = scope_error(".func f(x) {x*k}\n.param k={f(1)}").to_string();
    assert!(e.contains("undefined parameter 'k'"), "{e}");
    // Excluded numparam built-ins without a .func are not yet ported.
    let n = deck("r1 a 0 {agauss(1,2,3)}");
    let e = literalize(&n).unwrap_err();
    assert!(e.is_not_yet_ported(), "{e}");
    assert!(e.to_string().contains("f.cir:2:9"), "{e}");
    // Behavioural probe functions with no .func in scope: C rewrites such a
    // device into a behavioural one, which is not ported.
    for (body, at) in [
        ("v1 1 0 dc 1\nr1 1 0 {1/i(v1)}", "f.cir:3:11"),
        ("r1 1 0 {v(2)*2}", "f.cir:2:9"),
        ("r1 1 0 {2*V(2)}", "f.cir:2:11"),
    ] {
        let e = literalize(&deck(body)).unwrap_err();
        assert!(e.is_not_yet_ported(), "{body}: {e}");
        assert!(e.to_string().contains(at), "{body}: {e}");
        assert!(e.to_string().contains("b_transformation_wanted"), "{e}");
    }
    // ... but a .func named like one is an ordinary function.
    assert_eq!(probe(".func v(x) {x*3}", "v(2)"), 6.0);
    // Other undefined names stay invalid in device values too, as in C.
    let e = literalize(&deck("r1 1 0 {myfunc(1)}")).unwrap_err();
    assert!(!e.is_not_yet_ported(), "{e}");
    assert!(e.to_string().contains("undefined function 'myfunc'"), "{e}");
}

#[test]
fn single_quotes_are_braces_at_every_expression_site() {
    let n = deck(
        ".param a='1 + 2' b = 'a*2'\n\
         r1 n1 0 'b+1' tc1='a/1k'\n\
         c1 n1 0 1u ic='a'\n\
         v1 n1 0 dc 'a' ac '1' '0'\n\
         d1 n1 0 dm area='a'\n\
         .model dm d(is='1e-14*a')\n\
         .ic v(n1)='a'\n\
         .tran '1u*a' 1m",
    );
    let a = &n.params[0].assignments[0].expression;
    assert!(a.braced && a.quoted);
    assert_eq!(a.text, "1 + 2");
    assert_eq!(a.span.start.column, 11);
    assert_eq!(a.spelling(), "'1 + 2'");
    let r = &n.devices[0].parameters[0];
    assert_eq!(r.value, "'b+1'");
    assert!(matches!(&r.kind, ParameterKind::Expression(e) if e.quoted && e.text == "b+1"));
    assert!(matches!(
        &n.initial_conditions[0].entries[0].value,
        NodeHintValue::Expression(e) if e.quoted
    ));
    assert_eq!(n.analyses[0].expressions.len(), 1);
    let elaborated = literalize(&n).unwrap();
    let r = &elaborated.netlist.devices[0].parameters;
    assert_eq!(r[0].value, "7");
    assert_eq!(r[1].value, "3e-3");
    assert_eq!(elaborated.netlist.analyses[0].arguments[0], "3e-6");
    assert_eq!(elaborated.netlist.models[0].parameters[0].value, "3e-14");
    assert_eq!(
        elaborated.netlist.initial_conditions[0].entries[0].literal(),
        Some(3.0)
    );
    // X and .subckt parameter sites.
    let n = deck("x1 a b sub w='2*k'\n.subckt sub a b params: w='1+1'\n.ends");
    assert!(matches!(&n.devices[0].parameters[0].kind, ParameterKind::Expression(e) if e.quoted));
    assert!(
        matches!(&n.subcircuits[0].parameters[0].kind, ParameterKind::Expression(e) if e.quoted)
    );
    // Quoted and braced forms are the same expression.
    assert_eq!(probe(".param a='2*3'", "a"), probe(".param a={2*3}", "a"));
}

#[test]
fn malformed_and_unsupported_quotes_stay_explicit() {
    for (body, column, message) in [
        ("r1 a 0 '1+'", 11, "expected an operand"),
        ("r1 a 0 ''", 9, "empty expression"),
        (".param a='1+2", 10, "unterminated"),
        (".param a='{1}'", 11, "braces are not supported"),
    ] {
        match parse(body).expect_err(body) {
            SpiceError::Parse {
                location,
                message: m,
            } => {
                assert_eq!((location.line, location.column), (2, column), "{body}: {m}");
                assert!(m.contains(message), "{body}: {m}");
            }
            other => panic!("{body}: expected Parse, got {other}"),
        }
    }
    // Double-quoted strings are not expressions.
    for body in [".param s=\"x\"", "r1 a 0 \"x\"", ".model nm nmos level='2'"] {
        assert!(parse(body).unwrap_err().is_not_yet_ported(), "{body}");
    }
    // Quoted include paths are untouched.
    let n = deck(".include 'dir with space/a.inc'");
    assert_eq!(n.includes[0].path, "dir with space/a.inc");
}

#[test]
fn function_scope_api_resolves_lexically() {
    let n = deck(".func f(x) {x*2}\n.func g(x) {x+1}\n.subckt s a\n.func f(x) {x*5}\n.ends");
    let budget = EvalBudget::default();
    let root = FunctionScope::for_netlist(&n, &budget).unwrap();
    assert_eq!(root.definitions().len(), 2);
    let body = Arc::new(
        FunctionScope::new(
            Some(Arc::clone(&root)),
            &n.subcircuits[0].functions,
            &budget,
        )
        .unwrap(),
    );
    let (local, defined_in) = body.get("F").unwrap();
    assert_eq!(local.body.text, "x*5");
    assert!(std::ptr::eq(defined_in, body.as_ref()));
    let (outer, defined_in) = body.get("g").unwrap();
    assert_eq!(outer.body.text, "x+1");
    assert!(std::ptr::eq(defined_in, root.as_ref()));
    assert!(body.get("h").is_none());
    // The scope used by literalize and RunConfig exposes the functions.
    let scope = ParamScope::for_netlist(&n).unwrap();
    assert!(scope.functions().is_some_and(|f| f.get("f").is_some()));
    let expression = Parser::new()
        .parse_expression("f(2)+g(2)", &n.location)
        .unwrap();
    assert_eq!(
        scope
            .evaluate(&expression, &mut EvalBudget::default())
            .unwrap(),
        7.0
    );
    // The plain root scope has no functions.
    let plain = ParamScope::root(&n.params).unwrap();
    assert!(
        plain
            .evaluate(&expression, &mut EvalBudget::default())
            .is_err()
    );
}

#[test]
fn user_calls_are_syntax_until_evaluated() {
    let n = deck("r1 a 0 {foo(1, 2)}");
    let ParameterKind::Expression(e) = &n.devices[0].parameters[0].kind else {
        panic!("expression");
    };
    match &e.root.kind {
        ExprKind::UserCall { name, arguments } => {
            assert_eq!(name, "foo");
            assert_eq!(arguments.len(), 2);
        }
        other => panic!("{other:?}"),
    }
    assert!(e.references().is_empty());
}

fn round_trip(text: &str) -> String {
    let first = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("in.cir"), text))
        .unwrap();
    let written = write_netlist(&first).unwrap_or_else(|e| panic!("{e}\n{text}"));
    let second = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("out.cir"), &written))
        .unwrap_or_else(|e| panic!("{e}\n{written}"));
    if let Some(difference) = semantic_diff(&first, &second) {
        panic!("not equivalent: {difference}\n{written}");
    }
    assert!(semantic_eq(&first, &second));
    assert_eq!(write_netlist(&second).unwrap(), written, "fixed point");
    written
}

#[test]
fn writer_round_trips_functions_and_quotes() {
    let written = round_trip(
        "t\n.FUNC f( X , y )= 'x*y'\n.func g() {2}\n.func h(z) z + 1\n\
         .param a='1 + 2' b={f(a, 2)}\n\
         r1 n 0 'g()+h(a)' tc1='a/1k'\n\
         v1 n 0 dc 'a' ac '1' 0\n\
         .model dm d(is='1e-14')\n\
         d1 n 0 dm area='a'\n\
         .subckt s p params: w='a*2'\n.func k(q) {q*w}\nr1 p 0 {k(2)}\n.ends s\n\
         x1 n s w='3'\n\
         .ic v(n)='a'\n.tran '1u' 1m\n.end\n",
    );
    let expected = "t\n.func f(x,y) 'x*y'\n.func g() {2}\n.func h(z) z + 1\n\
        .param a='1 + 2' b={f(a, 2)}\n\
        r1 n 0 'g()+h(a)' tc1='a/1k'\n\
        v1 n 0 dc 'a' ac '1' 0\n\
        .model dm d(is='1e-14')\n\
        d1 n 0 dm area='a'\n\
        .subckt s p params: w='a*2'\n  .func k(q) {q*w}\n  r1 p 0 {k(2)}\n.ends s\n\
        x1 n s w='3'\n\
        .ic v(n)='a'\n.tran '1u' 1m\n.end\n";
    assert_eq!(written, expected);
    // The `.param` spelling of a function definition is kept.
    let written = round_trip(
        "t\n.param f(x)={x*3}\n.PARAM G( X , y ) = 'x+y'\n.param h()= x+1\n.param x=1\n\
         .subckt s a\n.param k(q)={q*2}\nr1 a 0 {k(f(1))}\n.ends\n.end\n",
    );
    assert_eq!(
        written,
        "t\n.param f(x)={x*3}\n.param g(x,y)='x+y'\n.param h()=x+1\n.param x=1\n\
         .subckt s a\n  .param k(q)={q*2}\n  r1 a 0 {k(f(1))}\n.ends s\n.end\n"
    );
    // A quoted value is not semantically the same spelling as a braced one,
    // but both evaluate the same.
    let quoted = deck(".param a='1'");
    let braced = deck(".param a={1}");
    assert!(!semantic_eq(&quoted, &braced));
}
